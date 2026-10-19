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
local K = {}
K.EN = {} -- a translated label -> its English, so label-matched things (menu icons) still work
local C = {
    bg = hex("121212"), panel = hex("1a1a1a"), raised = hex("242424"), line = hex("2e2e2e"), edge = hex("5a5a56"),
    text = hex("e8e8e6"), muted = hex("9b9b97"), accent = hex("6e080c"), accentHover = hex("842d30"), accentText = hex("fc4541"),
    hover = hex("3a1214"), banner = hex("322220"), onSelected = hex("ffffff"), dropEdge = hex("842d30"), ok = hex("3fb950"), warn = hex("f0883e"), danger = hex("e5604f"),
    black = hex("000000"), desc = hex("dcdcda"), onAccent = hex("ffffff"), grid = hex("ffffff"), focus = hex("ffffff", 0.85),
}
local function copy(c, a) return { r = c.r, g = c.g, b = c.b, a = a or c.a } end

-- the app's theme (app.txt: accent=#rrggbb, theme=dark|light): the palette is changed in place, so every
-- colour already held keeps pointing at it
local THEME = { key = "", rev = 0 }
local SHADOW = { k = 1 }
local GRID = { major = 0.09, minor = 0.035 }
local PALETTES = {
    dark = { bg = "121212", panel = "1a1a1a", raised = "242424", line = "2e2e2e", edge = "5a5a56", text = "e8e8e6", muted = "9b9b97",
        onSelected = "ffffff", danger = "e5604f", ok = "3fb950", warn = "f0883e", desc = "dcdcda", grid = "ffffff" },
    light = { bg = "e9ece8", panel = "f5f6f3", raised = "ffffff", line = "cdd4ce", edge = "9aa59d", text = "1b2124", muted = "59665f",
        onSelected = "1b2124", danger = "b2402f", ok = "1a7f37", warn = "bc4c00", desc = "2c3438", grid = "1b2124" },
}
local function setRgb(c, v) c.r, c.g, c.b = v.r, v.g, v.b end
--- `t` of the way from colour a to colour b.
local function mix(a, b, t) return { r = a.r + (b.r - a.r) * t, g = a.g + (b.g - a.g) * t, b = a.b + (b.b - a.b) * t } end
--- A bright red-ish in the accent's own hue, for text and thin marks (the app's oklch(0.66 0.22 h), roughly).
local function accentBright(a)
    local mx, mn = math.max(a.r, math.max(a.g, a.b)), math.min(a.r, math.min(a.g, a.b))
    local d, h = mx - mn, 0
    if d > 0.0001 then
        if mx == a.r then h = (a.g - a.b) / d elseif mx == a.g then h = (a.b - a.r) / d + 2 else h = (a.r - a.g) / d + 4 end
        if h < 0 then h = h + 6 end
    end
    local l = 0.62
    local c = (1 - math.abs(2 * l - 1)) * 0.96
    local x = c * (1 - math.abs(h - 2 * math.floor(h / 2) - 1))
    local m = l - c / 2
    local i = math.floor(h)
    local r, g, b = c, 0, x
    if i == 0 then r, g, b = c, x, 0 elseif i == 1 then r, g, b = x, c, 0 elseif i == 2 then r, g, b = 0, c, x elseif i == 3 then r, g, b = 0, x, c elseif i == 4 then r, g, b = x, 0, c end
    return { r = r + m, g = g + m, b = b + m }
end
--- Take the app's accent and theme (nil: the dark defaults). True when the palette changed.
local function applyTheme(app)
    local acc = app and app.accent and tostring(app.accent):match("^#?(%x%x%x%x%x%x)$")
    local light = app ~= nil and app.theme == "light"
    local key = (acc or "-") .. (light and "L" or "D")
    if key == THEME.key then return false end
    THEME.key, THEME.rev = key, THEME.rev + 1
    for k, h in pairs(light and PALETTES.light or PALETTES.dark) do setRgb(C[k], hex(h)) end
    SHADOW.k = light and 0.3 or 1
    -- the focus ring: white in the dark theme, the accent 75% mixed with black in the light one; map cells too
    C.focus.a = light and 1 or 0.85
    if K.MAP_CELL then setRgb(K.MAP_CELL, hex(light and "2f6fae" or "5b9bd5")) end
    GRID.major = 0.09
    if not acc and not light then
        for k, h in pairs({ accent = "6e080c", accentHover = "842d30", accentText = "fc4541", hover = "3a1214", banner = "322220", dropEdge = "842d30", focus = "ffffff" }) do setRgb(C[k], hex(h)) end
        return true
    end
    local a = hex(acc or "6e080c")
    setRgb(C.accent, a)
    setRgb(C.accentHover, mix(a, C.onAccent, 0.15))
    setRgb(C.accentText, light and mix(a, C.black, 0.3) or accentBright(a))
    setRgb(C.hover, mix(C.bg, a, 0.34))
    setRgb(C.banner, mix(C.panel, C.danger, 0.12))
    setRgb(C.dropEdge, C.accentHover)
    setRgb(C.focus, light and mix(C.black, a, 0.75) or hex("ffffff"))
    return true
end

--- This screen's words in the game's language (Translate/<LANG>/UI.json, UI_PZMM_<key>), else `en` as written;
--- %1, %2... are the extra arguments.
K.TX = {} -- each key's text, looked up once: the game logs an exception on every lookup of a %1 text
local function T(key, en, ...)
    local str = K.TX[key]
    if not str then
        str = en
        pcall(function()
            -- %1 comes back as itself: the game's loader turns it into Java's %1$s, which wants arguments
            local k = "UI_PZMM_" .. key
            local t = getText(k, "%1", "%2")
            if t and t ~= k then str = t end
        end)
        K.TX[key] = str
    end
    local args = { ... }
    if #args > 0 then str = str:gsub("%%(%d)", function(n) return tostring(args[tonumber(n)] or "") end) end
    if str ~= en then K.EN[str] = en end
    return str
end

--- t as local time for os.date, which the game gives in UTC (the offset is looked up once, DST and all)
function K.LT(t)
    if not K.tz then
        K.tz = 0
        pcall(function()
            local c = Calendar.getInstance()
            K.tz = math.floor((c:get(Calendar.ZONE_OFFSET) + c:get(Calendar.DST_OFFSET)) / 1000)
        end)
    end
    return (t or os.time()) + K.tz
end

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
local GAP = 14
local BOX = 16

--- Font heights and the sizes built on them, read again when the screen opens (after a resolution change,
--- or with Larger text, one font size up).
local function measure(large, compact)
    SMALL, MEDIUM = large and UIFont.Medium or UIFont.Small, large and UIFont.Large or UIFont.Medium
    H_SMALL, H_MEDIUM = TM:getFontHeight(SMALL), TM:getFontHeight(MEDIUM)
    BAR = H_SMALL + 14
    ROW2 = H_SMALL + 19
    TOP = ROW1 + ROW2
    FILTERS = BAR + 16
    ROW = compact and math.max(30, H_SMALL + 8) or math.max(40, H_SMALL + 16)
end

local function tex(name) return getTexture("media/ui/PZMM/" .. name .. ".png") end
K.LOGO = tex("logo")
-- the app's links (app/src/lib/Links.svelte), each brand's own logo
K.RELEASES_URL = "https://github.com/SEVNC3/pz-mod-manager/releases/latest"
local LINKS = {
    { tex("link_github"), "https://github.com/SEVNC3/pz-mod-manager", "GitHub", "GitHub\nThe PZMM app: download it, read the source, report a bug." },
    { tex("link_discord"), "https://discord.com/invite/6zKZuZ5WWe", "Discord", "Discord\nHelp, ideas and the PZMM community." },
    { tex("link_kofi"), "https://ko-fi.com/sevnc3", "Ko-fi", "Ko-fi\nBuy the developer a coffee: PZMM is free." },
}
-- the app's top bar actions the game can do too: { id, icon, tooltip }
local ACTIONS = {
    { "share", tex("icon_share"), "Copy mod list\nYour enabled mods as a Mod Manager preset (the game's Presets > Add takes it) or a PZMM share code.", T("act_share", "Share") },
    { "refresh", tex("icon_refresh"), "Rescan mods\nPicks up mods Steam downloaded, or you added, since this screen opened.", T("act_rescan", "Rescan") },
    { "folder", tex("icon_folder"), "Open mods folder\nZomboid/mods, where mods you made or downloaded by hand go.", T("act_folder", "Folder") },
}
-- the PZMM section labels, rendered in the app's style; any other name gets a tape drawn here
K.TAPE_L, K.TAPE_M, K.TAPE_R = tex("tape_l"), tex("tape_m"), tex("tape_r")
-- the app's list icons, white, drawn tinted: grip dots, fold chevrons, the hover marker and the drop feather
-- the app's toolbar icons (tb_<name>), white, drawn tinted
local TB = {}
for _, n in ipairs({ "layers", "filter", "sort", "plus", "pulse", "x", "undo", "redo", "check", "chev_up", "chev_down", "copy", "alert", "trash", "download", "play", "update", "overlap", "lock", "search", "tag", "folder", "star", "steam", "swap", "settings", "external", "link", "maximize", "minimize", "pencil", "eye", "server", "info" }) do TB[n] = tex("tb_" .. n) end
K.GRIP, K.CHEV_DOWN, K.CHEV_RIGHT, K.SEL_MARK, K.FEATHER = tex("grip"), tex("chev_down"), tex("chev_right"), tex("marker"), tex("feather")
local MARKER = (function()
    local ok, f = pcall(function() return UIFont.Handwritten end)
    return ok and f or UIFont.Medium
end)()
local H_MARKER = TM:getFontHeight(MARKER)
K.TAPE_CACHE = {}
local function tapeImage(name)
    if K.TAPE_CACHE[name] == nil then
        local slug = (name:lower():gsub("[^a-z0-9]+", "_"):gsub("^_+", ""):gsub("_+$", ""))
        K.TAPE_CACHE[name] = tex("tape_" .. slug) or false
    end
    return K.TAPE_CACHE[name]
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
    ui:drawTextureScaled(K.TAPE_L, x, y, 14, h, 1, 1, 1, 1)
    ui:drawTextureScaled(K.TAPE_M, x + 14, y, w, h, 1, 1, 1, 1)
    ui:drawTextureScaled(K.TAPE_R, x + 14 + w, y, 84, h, 1, 1, 1, 1)
    ui:drawText(name, x + 16, cy - TM:getFontHeight(MARKER) / 2, 0.165, 0.157, 0.137, 1, MARKER)
    return x + 98 + w
end
K.STAR_ON, K.STAR_OFF = getTexture("media/ui/inventoryPanes/FavouriteYes.png"), getTexture("media/ui/inventoryPanes/FavouriteNo.png")
K.FOLDER = tex("icon_folder")
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
K.CORNERS = {}
local function corners(r, kind)
    local key = r .. kind
    if not K.CORNERS[key] then
        local t = {}
        for _, p in ipairs({ "tl", "tr", "bl", "br" }) do t[p] = tex("c" .. r .. "_" .. kind .. "_" .. p) end
        K.CORNERS[key] = t
    end
    return K.CORNERS[key]
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
--- PZMM's Settings write the same file: it's read again every few seconds at the main menu, so a change
--- there shows here.
local SETTINGS = { logo = true, watermark = true, news = true, tips = true, large = false, ids = false, favfirst = false, compact = false }
local settingsAt = 0
local settingsChanged = false
local function loadSettings()
    settingsAt = getTimestampMs()
    local before = {}
    for k, v in pairs(SETTINGS) do before[k] = v end
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
    for k, v in pairs(SETTINGS) do if before[k] ~= v then settingsChanged = true end end
end
loadSettings()
settingsChanged = false
local function saveSettings(key)
    pcall(function()
        -- read it again first: PZMM may have changed other keys since; only the toggled one is ours
        local out = {}
        local r = getFileReader("pzmm/client.txt", false)
        if r then
            local line = r:readLine()
            while line do
                local k, v = line:match("^(%w+)=(%d)$")
                if k then out[k] = v == "1" end
                line = r:readLine()
            end
            r:close()
        end
        for k, v in pairs(SETTINGS) do
            if k == key or out[k] == nil then out[k] = v end
            SETTINGS[k] = out[k]
        end
        local w = getFileWriter("pzmm/client.txt", true, false)
        for k, v in pairs(out) do w:write(k .. "=" .. (v and "1" or "0") .. "\n") end
        w:close()
    end)
end

--- A hover tooltip, as the app's: set while drawing whatever is hovered, drawn last (in render) so
--- nothing covers it. Lines split on "\n"; the first is the bold title.
local function tip(ui, s) if SETTINGS.tips then ui.tipText = s end end
local function drawTip(ui)
    local s = ui.tipText
    ui.tipText = nil
    -- only after ~0.4 s over the same thing, as the app's
    if s ~= ui.tipShown then ui.tipShown, ui.tipAt = s, getTimestampMs() end
    if not s or getTimestampMs() - ui.tipAt < 400 then return end
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
--- A panel as the app's cards: no outline, a soft shadow under it.
local function card(ui, x, y, w, h)
    round(ui, x - 1, y + 1, w + 2, h + 4, 8, C.black, 0.28 * SHADOW.k)
    round(ui, x - 2, y + 2, w + 4, h + 8, 8, C.black, 0.14 * SHADOW.k)
    round(ui, x, y, w, h, 8, C.panel)
end

-- scrollbars: one look for all. Registered while drawn (ui.bars), so a press can pick the thumb up;
-- dragging moves ui[key] (the scroll offset) and the caller's clamp does the rest.
local function pillBar(ui, x, y, w, h, c, a)
    if w >= 6 then return round(ui, x, y, w, h, 3, c, a) end
    rect(ui, x, y + 1, w, h - 2, c, a)
    rect(ui, x + 1, y, w - 2, h, c, a)
end
--- A bar for the box x, y, w, h whose content is `full` px tall, scrolled `ui[key]`.
local function scrollbar(ui, key, x, y, w, h, full)
    if full <= h then return end
    local th = math.max(30, h * h / full)
    local ty = math.floor(y + (h - th) * ui[key] / (full - h))
    local mx, my = ui:getMouseX(), ui:getMouseY()
    local d = ui.sbDrag
    if d and d.key == key then
        ui[key] = math.max(0, math.min(full - h, (my - d.off - y) / (h - th) * (full - h)))
        ty = math.floor(y + (h - th) * ui[key] / (full - h))
    end
    local hot = (d and d.key == key) or (not d and mx >= x + w - 14 and mx < x + w and my >= ty and my < ty + th)
    local bw = hot and 6 or 4
    pillBar(ui, x + w - 8 - (bw - 4) / 2, ty, bw, math.floor(th), C.text, hot and 0.34 or 0.16)
    ui.bars = ui.bars or {}
    ui.bars[key] = { x = x + w - 14, y = ty, w = 14, h = th, top = y }
end
--- A press at x, y on a registered thumb starts dragging it.
local function barDown(ui, x, y)
    -- (a thumb scrolled under the header, the filters bar or the footer is out of reach)
    if ui.page then
        local top = ui.page == "mods" and TOP + FILTERS or (ui.page == "home" and TOP or TOP + 12)
        if y < top or y >= ui.height - BOTTOM then return false end
    end
    for key, b in pairs(ui.bars or {}) do
        if x >= b.x and x < b.x + b.w and y >= b.y and y < b.y + b.h then
            ui.sbDrag = { key = key, off = y - b.y }
            return true
        end
    end
    return false
end

--- A button drawn in place (for lists that scroll), in our buttons' colours: "primary" red, else raised. `icon`
--- (a tb_ texture) goes before the label; `fw` makes it that wide, its content centred.
local function flatButton(ui, label, x, y, mx, my, kind, icon, fw)
    local cw = width(label) + (icon and 20 or 0)
    local w, h = fw or (cw + 28), BAR
    local over = mx >= x and mx < x + w and my >= y and my < y + h
    local sx = fw and (x + math.floor((w - cw - 1) / 2)) or (x + 13)
    local lx = sx + (icon and 20 or 1)
    local ty = y + (h - H_SMALL) / 2
    -- quiet at rest (no fill, no outline); the primary one is blood red with a bold label
    if kind == "primary" then
        round(ui, x, y, w, h, 6, over and C.accentHover or C.accent)
        outline(ui, x, y, w, h, 6, C.accent)
        if icon then ui:drawTextureScaled(icon, sx, y + math.floor((h - 14) / 2), 14, 14, 1, C.onAccent.r, C.onAccent.g, C.onAccent.b) end
        bold(ui, label, lx, ty, C.onAccent)
    else
        if over then
            round(ui, x, y, w, h, 6, C.hover)
            outline(ui, x, y, w, h, 6, C.accent, 0.7)
        end
        local c = over and C.onSelected or C.text
        if icon then ui:drawTextureScaled(icon, sx, y + math.floor((h - 14) / 2), 14, 14, 1, c.r, c.g, c.b) end
        text(ui, label, lx, ty, c)
    end
    return w, h, over
end

--- A quiet toolbar button as the app's: an icon, then a label (none: a square icon button). `on` keeps it lit
--- (its menu open, its filter on); `off` greys it out. Returns its width and whether the mouse is over it.
local function toolButton(ui, icon, label, x, y, mx, my, on, off)
    local w = label and width(label) + 46 or BAR
    local over = not off and mx >= x and mx < x + w and my >= y and my < y + BAR
    if over then
        round(ui, x, y, w, BAR, 6, C.hover)
        outline(ui, x, y, w, BAR, 6, C.accent, 0.7)
    end
    -- lit (its menu open, its filter on) is only the text colour, as the app's
    local c = over and C.onSelected or on and C.accentText or C.text
    local a = off and 0.5 or 1
    local sz = label and 14 or 16
    local ix = label and x + 13 or x + math.floor((BAR - 16) / 2)
    if icon then ui:drawTextureScaled(icon, ix, y + math.floor((BAR - sz) / 2), sz, sz, a, c.r, c.g, c.b) end
    if label then ui:drawText(label, x + 33, y + (BAR - H_SMALL) / 2, c.r, c.g, c.b, a, SMALL) end
    return w, over
end

--- A chip ("Needed by 3"); `c` outlined in that colour, else a dark chip. `lit` (on a lit row) turns its text white,
--- the border stays; `edge` is a border of its own (the tone then only colours the text). Returns its right edge.
--- A capsule's corner radius for height h: the largest corner texture that fits in half of it.
local function capR(h)
    local r = 3
    for _, k in ipairs({ 4, 6, 8, 9, 10, 16 }) do if k * 2 <= h then r = k end end
    return r
end
local function chip(ui, label, x, y, c, lit, edge, ea)
    local w, h = width(label) + 11, H_SMALL + 2
    -- no fill: a coloured outline, or the plain line colour with bold text
    outline(ui, x, y, w, h, capR(h), edge or c or C.line, ea or ((edge or not c) and 1 or 0.5))
    bold(ui, label, x + 5, y + 1, lit and C.onSelected or c or C.text)
    return x + w
end
--- The player's own tag: a capsule, transparent, a line border, normal weight.
local function tagChip(ui, label, x, y, lit)
    local w, h = width(label) + 17, H_SMALL + 2
    -- transparent, a line border, normal weight (the app's tag chip)
    outline(ui, x, y, w, h, capR(h), C.line, 1)
    text(ui, label, x + 8, y + 1, lit and C.onSelected or C.text)
    return x + w
end
local function tick(ui, x, y, c)
    for i, d in ipairs({ { 0, 0 }, { 2, 2 }, { 4, 0 }, { 6, -2 }, { 8, -4 } }) do rect(ui, x + d[1], y + d[2], 2, 2, c) end
end
--- A source pill ending at `right` ("Subscribed" in green with a tick, as in the app); returns its left edge. `lit`
--- (a lit row) whitens only the text: the tone border stays. `hot` is the hovered state.
local function pill(ui, label, right, y, c, ticked, lit, tall, icon, hot)
    local w, h = width(label) + ((ticked or icon) and 30 or 18) + (tall and 8 or 0), tall and BAR or (H_SMALL + 4)
    local x = right - w
    local r = capR(h)
    local oc = c == C.muted and C.line or c
    -- tall: the tone as a faint fill under a stronger border
    if tall then round(ui, x, y, w, h, r, oc, 0.08) end
    if hot then round(ui, x, y, w, h, r, C.hover) end
    outline(ui, x, y, w, h, r, hot and C.accent or oc, hot and 0.7 or (oc == C.line and 1 or (lit and 0.7 or 0.45)))
    local tc = lit and C.onSelected or c
    local tx = x + 9 + (tall and 4 or 0)
    if icon then
        ui:drawTextureScaled(icon, tx, y + math.floor((h - 12) / 2), 12, 12, 1, tc.r, tc.g, tc.b)
        tx = tx + 16
    elseif ticked then
        tick(ui, tx, y + h / 2, tc)
        tx = tx + 14
    end
    -- an update waiting is bold
    local f = icon == TB.update and bold or text
    f(ui, label, tx, y + (h - H_SMALL) / 2, tc)
    return x
end
local function handle(ui, x, y)
    if K.GRIP then return ui:drawTextureScaled(K.GRIP, x - 4, y - 1, 14, 14, 1, C.muted.r, C.muted.g, C.muted.b) end
    for i = 0, 2 do
        rect(ui, x, y + i * 5, 2, 2, C.muted, 0.6)
        rect(ui, x + 5, y + i * 5, 2, 2, C.muted, 0.6)
    end
end
local function checkbox(ui, x, y, on, bad)
    if on then
        round(ui, x, y, BOX, BOX, 3, C.accent)
        tick(ui, x + 3, y + 8, C.onAccent)
    else
        round(ui, x, y, BOX, BOX, 3, C.raised)
        outline(ui, x, y, BOX, BOX, 3, bad and C.danger or C.edge)
    end
end

--- A details heading: small bold muted capitals. Returns the y under it.
function K.heading(ui, s, x, y)
    bold(ui, s:upper(), x, y, C.muted)
    return y + H_SMALL + 6
end
--- A text box's words in the palette's colours (the game draws them white: unreadable on the light theme).
function K.tint(e)
    if not e or e.pzmmTheme == THEME.rev then return end
    e.pzmmTheme = THEME.rev
    pcall(function() e:setTextRGBA(C.text.r, C.text.g, C.text.b, 1) end)
    pcall(function() e:setPlaceholderTextRGBA(C.muted.r, C.muted.g, C.muted.b, 1) end)
end
--- The one search box is My mods', Maps' or Servers': each keeps its own words (Maps and Servers read theirs from
--- mapQuery and serverFilter; My mods' are put away while they're away).
function K.swapSearch(ui)
    local want = (ui.page == "maps" or ui.page == "servers") and ui.page or "mods"
    if want == ui.searchOwner then return end
    if ui.searchOwner == "mods" then ui.modsText = ui.search:getText() end
    ui.searchOwner = want
    local words, hint = ui.modsText or "", "Search mods  ( / )"
    if want == "maps" then
        words, hint = ui.mapQuery or "", "Search map mods"
    elseif want == "servers" then
        words, hint = ui.serverFilter or "", "Search servers"
    end
    ui.search:setText(words)
    pcall(function() ui.search:unfocus() end)
    pcall(function() ui.search.javaObject:setPlaceholderText(hint) end)
    if want == "mods" then ui:updateView() end
end
--- The search box inside a page's bar (Maps, Servers) at x, y, w: its box, icon and the entry over them (shown
--- only while the bar is in the frame top .. top + bh). Returns w.
function K.searchSpot(ui, x, y, w, top, bh)
    round(ui, x, y, w, BAR, 6, C.raised)
    local focused = false
    pcall(function() focused = ui.search:isFocused() end)
    if focused then
        outline(ui, x, y, w, BAR, 6, C.focus)
        outline(ui, x - 1, y - 1, w + 2, BAR + 2, 6, C.focus)
    else
        outline(ui, x, y, w, BAR, 6, C.line)
    end
    if TB.search then ui:drawTextureScaled(TB.search, x + 10, y + math.floor((BAR - 14) / 2), 14, 14, 1, C.muted.r, C.muted.g, C.muted.b) end
    local e = ui.search
    e:setX(x + 32)
    e:setY(y + 2)
    e:setWidth(w - 40)
    e:setVisible(y >= top and y + BAR <= top + bh)
    return w
end

-- --- buttons: rounded, in the app's colours (only ours: the game's other buttons keep theirs)

local buttonPrerender = ISButton.prerender
--- A button's colours from the palette: "primary" (blood red), else quiet. Redone when the theme changes.
local function paintButton(b)
    local kind = b.pzmmKind
    b.pzmmTheme = THEME.rev
    b.backgroundColor = kind == "primary" and copy(C.accent) or copy(C.hover, 0)
    b.backgroundColorMouseOver = kind == "primary" and copy(C.accentHover) or copy(C.hover)
    b.borderColor = kind == "primary" and copy(C.accent) or copy(C.accent, 0)
    b.borderColorOver = kind == "primary" and copy(C.accent) or copy(C.accent, 0.7)
    b.textColor = copy(kind == "primary" and C.onAccent or C.text)
    if kind ~= "primary" then
        b.textColorRest, b.textColorOver = b.textColor, copy(C.onSelected)
    end
end
function ISButton:prerender()
    if not self.pzmmRound or self.isJoypad then return buttonPrerender(self) end
    if self.pzmmTheme ~= THEME.rev then paintButton(self) end
    self.fade:setFadeIn((self.mouseOver and self:isMouseOver()) and self.enable or false)
    self.fade:update()
    local f = self.pressed and 1 or self.fade:fraction()
    local a, b = self.backgroundColor, self.backgroundColorMouseOver
    local c = { r = b.r * f + a.r * (1 - f), g = b.g * f + a.g * (1 - f), b = b.b * f + a.b * (1 - f), a = (b.a * f + a.a * (1 - f)) * (self.enable and 1 or 0.5) }
    if c.a > 0 then round(self, 0, 0, self.width, self.height, 6, c) end
    -- the border and the label fade to their hover colours too (quiet buttons light up on hover)
    local bc, bo = self.borderColor, self.borderColorOver or self.borderColor
    local bcol = { r = bo.r * f + bc.r * (1 - f), g = bo.g * f + bc.g * (1 - f), b = bo.b * f + bc.b * (1 - f), a = (bo.a * f + bc.a * (1 - f)) * (self.enable and 1 or 0.5) }
    if bcol.a > 0 then outline(self, 0, 0, self.width, self.height, 6, bcol) end
    if self.textColorOver then self.textColor = f > 0.5 and self.textColorOver or self.textColorRest end
    -- bold, as the game's font has none: the label drawn twice, a pixel apart (render draws the first)
    if self.pzmmBold and self.title then
        local t = self.textColor
        self:drawTextCentre(self.title, self.width / 2 + 1, self.height / 2 - H_SMALL / 2, t.r, t.g, t.b, t.a * (self.enable and 1 or 0.5), SMALL)
    end
    self:updateTooltip()
end

--- `kind`: "primary" (blood red), else quiet: no fill or outline until hovered.
local function button(parent, x, y, label, kind, onClick, w)
    local b = ISButton:new(x, y, w or (width(label) + 28), BAR, label, parent, onClick)
    b:initialise()
    b:instantiate()
    b.pzmmRound = true
    b.pzmmBold = kind == "primary"
    b.pzmmKind = kind
    paintButton(b)
    parent:addChild(b)
    return b
end

-- --- what's shown: the game's mod data, in sections, filtered by the tab and the search

--- Workshop items of mods PZMM installed (installed.txt column 3; they sit in Zomboid/mods, so the game has no Workshop ID for them).
local installedItem = {}
--- A mod's Workshop item ID as a string: the game's, else the one PZMM installed it from; "" when it has none.
local function wsOf(d)
    local ws
    pcall(function() ws = d.modInfo:getWorkshopID() end)
    ws = tostring(ws or "")
    if ws == "" or ws == "0" then ws = installedItem[d.modId] or "" end
    return ws
end
--- PZMM's own copy of a Workshop item: the game has no Workshop ID for the mod, but installed.txt has its item.
local function pzmmCopy(d)
    local ws
    pcall(function() ws = d.modInfo:getWorkshopID() end)
    ws = tostring(ws or "")
    return (ws == "" or ws == "0") and (installedItem[d.modId] or "") ~= ""
end
--- 1 Subscribed (Steam's), 2 on this PC, 3 can't load on this build, 4 Workshop (PZMM's copy).
local function source(d)
    if not d.isAvailable then return 3 end
    if pzmmCopy(d) then return 4 end
    return wsOf(d) ~= "" and 1 or 2
end
K.SOURCES = { "Subscribed", "On this PC", "Can't load", "Workshop" }
--- A to Z by name, ignoring case; the mod ID settles equal names.
local function byName(a, b)
    local la, lb = (a.name or ""):lower(), (b.name or ""):lower()
    if la ~= lb then return not string.sort(la, lb) end
    return not string.sort(a.modId, b.modId)
end

--- A library from its lines: its profile's name, its sections in order, and each mod's section and place.
local function parseLibrary(lines)
    local lib = { sections = {}, of = {}, pos = {}, lines = {}, count = {}, header = {} }
    local section, n = nil, 0
    for _, line in ipairs(lines) do
        lib.lines[#lib.lines + 1] = line
        local profile = line:match("^# PZMM profile: (.*)$")
        local name = line:match("^# %-%-%-%s*(.*)$")
        if profile then
            lib.profile = profile
        elseif name then
            section = name ~= "" and name or "Other"
            lib.sections[#lib.sections + 1] = section
            lib.count[section] = lib.count[section] or 0
            lib.header[section] = true
        else
            local id = line:match("^[+-](.+)$")
            if id then
                n = n + 1
                lib.of[id], lib.pos[id] = section or "Other", n
                lib.count[section or "Other"] = (lib.count[section or "Other"] or 0) + 1
            end
        end
    end
    return lib
end

--- The active PZMM profile's library (Zomboid/Lua/pzmm/library.txt, which the app writes). nil without one.
local function readLibrary()
    local ok, lines = pcall(function()
        local r = getFileReader("pzmm/library.txt", false)
        if not r then return nil end
        local lines, line = {}, r:readLine()
        while line do
            lines[#lines + 1] = line
            line = r:readLine()
        end
        r:close()
        return lines
    end)
    return ok and lines and parseLibrary(lines) or nil
end

--- Mods you hid: Mod Manager's list in Zomboid/Lua/saved_modlists.txt (mmHidden:A;B), which PZMM reads and
--- writes too. Returns the set and the file's lines (to write back).
local function readHidden()
    local set, lines = {}, {}
    pcall(function()
        local r = getFileReader("saved_modlists.txt", false)
        if not r then return end
        local line = r:readLine()
        while line do
            lines[#lines + 1] = line
            local ids = line:match("^mmHidden:(.*)$")
            if ids then for id in ids:gmatch("[^;]+") do set[id] = true end end
            line = r:readLine()
        end
        r:close()
    end)
    return set, lines
end

--- Write the hidden set back, keeping the file's other lines (presets, favourites).
local function writeHidden(set)
    local _, lines = readHidden()
    local ids = {}
    for id in pairs(set) do ids[#ids + 1] = id end
    table.sort(ids)
    local out, done = {}, false
    for _, l in ipairs(lines) do
        if l:match("^mmHidden:") then
            if #ids > 0 then out[#out + 1] = "mmHidden:" .. table.concat(ids, ";") end
            done = true
        else
            out[#out + 1] = l
        end
    end
    if #out == 0 then out[1] = "VERSION=2" end
    if not done and #ids > 0 then out[#out + 1] = "mmHidden:" .. table.concat(ids, ";") end
    pcall(function()
        local w = getFileWriter("saved_modlists.txt", true, false)
        w:write(table.concat(out, "\n") .. "\n")
        w:close()
    end)
end

--- A tab-separated line's fields (empty ones kept).
local function fields(line)
    local p, start = {}, 1
    while true do
        local i = line:find("\t", start, true)
        if not i then p[#p + 1] = line:sub(start); return p end
        p[#p + 1] = line:sub(start, i - 1)
        start = i + 1
    end
end

--- An activity file's lines: { time, kind, text }, oldest first.
local function readActivity(file)
    local out = {}
    pcall(function()
        local r = getFileReader("pzmm/" .. file, false)
        if not r then return end
        local line = r:readLine()
        while line do
            local f = fields(line)
            local t = tonumber(f[1])
            if t and f[3] then out[#out + 1] = { time = t, kind = f[2] ~= "" and f[2] or "info", text = f[3] } end
            line = r:readLine()
        end
        r:close()
    end)
    return out
end

--- PZMM's profiles (Zomboid/Lua/pzmm/profiles.txt): the active one, then each with its kind. nil without it.
local function readProfiles()
    local ok, out = pcall(function()
        local r = getFileReader("pzmm/profiles.txt", false)
        if not r then return nil end
        local out = { list = {} }
        local line = r:readLine()
        local first = true
        while line do
            local f = fields(line)
            -- only the first line says which is active: a later "active" is a profile with that name
            if first and f[1] == "active" then out.active = f[2] elseif f[1] ~= "" then out.list[#out.list + 1] = { name = f[1], kind = f[2] or "" } end
            first = false
            line = r:readLine()
        end
        r:close()
        return out
    end)
    return ok and out or nil
end

--- PZMM's save backups (Zomboid/Lua/pzmm/backups.txt): "mode/save" -> { { file, at, slot }, ... }, newest first.
local function readBackups()
    local by = {}
    pcall(function()
        local r = getFileReader("pzmm/backups.txt", false)
        if not r then return end
        local line = r:readLine()
        while line do
            local f = fields(line)
            if f[2] and f[2] ~= "" then
                by[f[1]] = by[f[1]] or {}
                table.insert(by[f[1]], { file = f[2], at = tonumber(f[3]) or 0, slot = f[4] or "" })
            end
            line = r:readLine()
        end
        r:close()
    end)
    for _, list in pairs(by) do table.sort(list, function(a, b) return a.at > b.at end) end
    return by
end

--- PZMM's tags for each mod (Zomboid/Lua/pzmm/tags.txt: "<mod id>\t<tag1>;<tag2>"): id -> { tags }.
local function readTags()
    local by = {}
    pcall(function()
        local r = getFileReader("pzmm/tags.txt", false)
        if not r then return end
        local line = r:readLine()
        while line do
            local id, list = line:match("^([^\t]+)\t(.*)$")
            if id then
                local t = {}
                for x in list:gmatch("[^;]+") do t[#t + 1] = x end
                if #t > 0 then by[id] = t end
            end
            line = r:readLine()
        end
        r:close()
    end)
    return by
end

--- When PZMM installed each mod (Zomboid/Lua/pzmm/installed.txt: "<mod id>\t<secs>\t<Workshop item>"): id -> secs, and whether any; the items go to installedItem.
local function readInstalled()
    local by, any, items = {}, false, {}
    pcall(function()
        local r = getFileReader("pzmm/installed.txt", false)
        if not r then return end
        local line = r:readLine()
        while line do
            local id, at, item = line:match("^([^\t]+)\t(%d+)\t?(%d*)")
            if id then
                by[id], any = tonumber(at), true
                if item and item ~= "" and item ~= "0" then items[id] = item end
            end
            line = r:readLine()
        end
        r:close()
    end)
    installedItem = items
    return by, any
end

--- When each enabled mod first arrived on this PC, as the app knows it (Zomboid/Lua/pzmm/firstseen.txt, "<id>\t<secs>":
--- PZMM's install time, else when its folder appeared; it can reset if Steam downloads the mod again). id -> secs;
--- empty without the app (the game can't read folder dates).
function K.readFirstSeen()
    local by = {}
    pcall(function()
        local r = getFileReader("pzmm/firstseen.txt", false)
        if not r then return end
        local line = r:readLine()
        while line do
            local id, at = line:match("^([^\t]+)\t(%d+)")
            if id and tonumber(at) and tonumber(at) > 0 then by[id] = tonumber(at) end
            line = r:readLine()
        end
        r:close()
    end)
    return by
end

--- The search for the broken mod (Zomboid/Lua/pzmm/bisect.txt, so it survives restarting the game): the mods that were
--- on, in load order, the suspect groups, the groups under test and the round; or just a result (the mod or group
--- found, members split by "|"; "" for none). Groups are "a|b;c": a file from before groups reads as one mod a group.
local function readBisect()
    local b
    pcall(function()
        local r = getFileReader("pzmm/bisect.txt", false)
        if not r then return end
        local t = {}
        local line = r:readLine()
        while line do
            local k, v = line:match("^(%a+)\t(.*)$")
            if k then t[k] = v end
            line = r:readLine()
        end
        r:close()
        local function ids(s)
            local out = {}
            for id in tostring(s or ""):gmatch("[^;]+") do out[#out + 1] = id end
            return out
        end
        local function groups(s)
            local out = {}
            for _, g in ipairs(ids(s)) do
                local members = {}
                for id in g:gmatch("[^|]+") do members[#members + 1] = id end
                if #members > 0 then out[#out + 1] = members end
            end
            return out
        end
        if t.result ~= nil then
            b = { result = t.result }
        elseif t.original and t.suspects then
            b = { original = ids(t.original), suspects = groups(t.suspects), testing = groups(t.testing), round = tonumber(t.round) or 1, had = {}, maps = t.maps and ids(t.maps) or nil, alone = t.alone == "1" or nil, keep = {}, lead = tonumber(t.lead) }
            for _, id in ipairs(ids(t.keep)) do b.keep[id] = true end
            -- saved before groups: its frameworks stayed on as non-suspects, and stay on (alone round too)
            if t.keep == nil then
                local suspect = {}
                for _, g in ipairs(b.suspects) do for _, id in ipairs(g) do suspect[id] = true end end
                for _, id in ipairs(b.original) do if not suspect[id] then b.keep[id] = true end end
            end
            for _, id in ipairs(b.original) do b.had[id] = true end
        end
    end)
    return b
end
--- Keep `b` (nil: forget it).
local function writeBisect(b)
    pcall(function()
        local w = getFileWriter("pzmm/bisect.txt", true, false)
        if b and b.result ~= nil then
            w:write("result\t" .. b.result .. "\n")
        elseif b then
            local function groups(list)
                local out = {}
                for i, g in ipairs(list) do out[i] = table.concat(g, "|") end
                return table.concat(out, ";")
            end
            w:write("original\t" .. table.concat(b.original, ";") .. "\nsuspects\t" .. groups(b.suspects) .. "\ntesting\t" .. groups(b.testing) .. "\nround\t" .. b.round .. (b.maps and ("\nmaps\t" .. table.concat(b.maps, ";")) or "") .. (b.alone and "\nalone\t1" or "") .. (b.lead and ("\nlead\t" .. b.lead) or "") .. "\nkeep\t" .. table.concat((function() local k = {} for id in pairs(b.keep or {}) do k[#k + 1] = id end table.sort(k) return k end)(), ";") .. "\n")
        end
        w:close()
    end)
end

--- Now, in whole seconds since 1970 (Kahlua's os.time() can have a fraction).
local function now() return math.floor(os.time()) end

--- The PZMM app, when it's installed: it writes Zomboid/Lua/pzmm/app.txt (version, running_at, profile, kind)
--- every 5 s while it runs, so `running` is whether it's running now. nil when it never has.
local function readApp()
    local ok, app, gone = pcall(function()
        local r = getFileReader("pzmm/app.txt", false)
        if not r then return nil, true end
        local app = {}
        local line = r:readLine()
        while line do
            local k, v = line:match("^([%w_]+)=(.*)$")
            if k then app[k] = v end
            line = r:readLine()
        end
        r:close()
        if not app.version then return nil end
        local at = tonumber(app.running_at or "")
        app.running = at ~= nil and now() - at < 15
        return app
    end)
    return ok and app or nil, ok and gone or false
end

applyTheme((readApp()))
local PAGES = {
    { "home", T("tab_home", "Home"), T("tab_home_tip", "Home\nYour mods, problems and updates at a glance, and Continue."), "drawHome" },
    { "mods", T("tab_mods", "My mods"), T("tab_mods_tip", "My mods\nTurn mods on and off, sort them into your profile's sections, edit the load order and Check setup.") },
    { "feed", T("tab_feed", "What's new"), T("tab_feed_tip", "What's new\nChange notes for your Workshop mods updated since you last played.") },
    { "workshop", T("tab_workshop", "Browse Workshop"), T("tab_workshop_tip", "Browse Workshop\nSearch the Workshop in Steam, and look up any item or collection by its ID or link."), "drawWorkshop" },
    { "maps", T("tab_maps", "Maps"), T("tab_maps_tip", "Maps\nYour maps' load order, clashes and Autosort."), "drawMaps" },
    { "servers", T("tab_servers", "Servers"), T("tab_servers_tip", "Servers\nFind servers with their mods checked against yours, join, and give your hosted servers your mods."), "drawServers" },
    { "saves", T("tab_saves", "Saves"), T("tab_saves_tip", "Saves\nPlay a save, see its mods, pick its character, rename it or reset part of its world."), "drawSaves" },
    { "java", T("tab_java", "Java mods"), T("tab_java_tip", "Java mods\nZombieBuddy, and the mods that ship Java (and any missing their jar)."), "drawJava" },
    { "make", T("tab_make2", "Make a mod"), T("tab_make_tip2", "Make a mod\nYour own Workshop items: open their files, and upload an update."), "drawMake" },
    { "settings", T("tab_settings", "Settings"), T("tab_settings_tip", "Settings\nThis in-game mod manager's options, your own Workshop items, and the PZMM app."), "drawSettings" },
}
local PAGE = {}
for _, pg in ipairs(PAGES) do PAGE[pg[1]] = pg end
local TABS = { "All", "Enabled", "Disabled", "Problems" }
K.TAB_TIPS = {
    All = T("filter_all_tip", "All\nEvery mod the game found."),
    Enabled = T("filter_enabled_tip", "Enabled\nMods that load with your next game."),
    Disabled = T("filter_disabled_tip", "Disabled\nMods you have but aren't loading."),
    Problems = T("filter_problems_tip", "Problems\nMods that can't load on this build of the game, or clash with another mod."),
}
K.TAB_NAMES = { All = T("filter_all", "All"), Enabled = T("filter_enabled", "Enabled"), Disabled = T("filter_disabled", "Disabled"), Problems = T("filter_problems", "Problems") }
--- Required mods that aren't installed at all (set by updateView: id -> true).
local MISSING = {}
local function missingReqs(d)
    local out = {}
    for id, _ in pairs(d.requireMods or {}) do if MISSING[id] then out[#out + 1] = id end end
    return out
end
--- The sections you folded, kept in Zomboid/Lua/pzmm/collapsed.txt (one name a line).
function K.readCollapsed()
    local out = {}
    pcall(function()
        local r = getFileReader("pzmm/collapsed.txt", false)
        if not r then return end
        local line = r:readLine()
        while line do
            if line ~= "" then out[line] = true end
            line = r:readLine()
        end
        r:close()
    end)
    return out
end
function K.writeCollapsed(set)
    pcall(function()
        local w = getFileWriter("pzmm/collapsed.txt", true, false)
        for name, on in pairs(set) do if on then w:write(name .. "\n") end end
        w:close()
    end)
end
-- the enabled mods with a load order problem (set by updateView: id -> true), and what a mod that isn't installed has instead of mod info
K.PROB = {}
K.NOINFO = setmetatable({}, { __index = function() return function() return nil end end })
local SHOWN = {
    Enabled = function(d) return d.isActive end,
    Disabled = function(d) return not d.isActive and d.isAvailable end,
    -- as the app counts them: enabled mods with a requirement or load rule problem, or an enabled mod they clash
    -- with; Can't load, Build 41 only and the like are their own badges
    Problems = function(d) return d.isActive and (d.isIncompatible or #missingReqs(d) > 0 or K.PROB[d.modId] == true) end,
}

--- Steam's word for an item's download state, as we show it: { chip, colour, tooltip } (nil: nothing to say).
K.STEAM_STATES = {
    Downloading = { "Downloading", "warn", "Downloading\nSteam is still downloading it. Wait for it to finish before you load a save." },
    DownloadPending = { "Download waiting", "warn", "Download waiting\nSteam hasn't started downloading it yet." },
    NeedsUpdate = { "Update available", "warn", "Update available\nSteam has a newer version it hasn't downloaded yet. Restart Steam or the game to get it." },
    Error = { "Steam error", "danger", "Steam error\nSteam couldn't download it. Try Unsubscribe and Subscribe again in Steam." },
}

K.PZMM_UPDATE = { "Update available", "warn", "Update available\nA newer version is on the Workshop: Update gets it with PZMM" }
--- The chip, colour and tip for a Steam state; PZMM's own invented update says Update, not "restart Steam".
function K.look(st)
    if not st then return nil end
    if st.state == "NeedsUpdate" and st.pzmm then return K.PZMM_UPDATE end
    return K.STEAM_STATES[st.state]
end

PZMMMods = ISPanelJoypad:derive("PZMMMods")

function PZMMMods:new(x, y, w, h)
    local o = ISPanelJoypad:new(x, y, w, h)
    setmetatable(o, self)
    self.__index = self
    o.background = false
    o.model = ModSelector.Model:new(o)
    o.tab = "All"
    o.collapsed = K.readCollapsed()
    o.entries = {}
    o.scroll, o.feedScroll = 0, 0
    o.feedOpen, o.feedMore = {}, {}
    o.selected = nil
    o.needed, o.place = {}, {}
    return o
end

function PZMMMods:createChildren()
    -- the box is drawn behind it (rounded); the field itself is clear, inset from the edges
    self.search = ISTextEntryBox:new("", PAD + 8, TOP + 8 + 4, 180 - 12, BAR - 4)
    self.search.font = SMALL
    self.search:initialise()
    self.search:instantiate()
    self.search.backgroundColor = copy(C.raised, 0)
    self.search.borderColor = copy(C.line, 0)
    self.search.onTextChange = function()
        local txt = self.search:getText()
        self.searchSeen = txt
        -- "/" focused the box: that very key must not land in it
        if self.slashAt and getTimestampMs() - self.slashAt < 250 then
            self.slashAt = nil
            if txt:sub(-1) == "/" then
                txt = txt:sub(1, -2)
                self.search:setText(txt)
            end
        end
        if self.searchOwner == "maps" then
            self.mapQuery = txt ~= "" and txt or nil
            return
        end
        if self.searchOwner == "servers" then
            self.serverFilter = txt ~= "" and txt or nil
            return
        end
        self.scroll = 0
        self:updateView()
    end
    pcall(function() self.search.javaObject:setPlaceholderText("Search mods  ( / )") end)
    self.searchOwner = "mods"
    self:addChild(self.search)
    self.homeSearch = ISTextEntryBox:new("", 0, 0, 100, BAR - 4)
    self.homeSearch.font = SMALL
    self.homeSearch:initialise()
    self.homeSearch:instantiate()
    self.homeSearch.backgroundColor = copy(C.raised, 0)
    self.homeSearch.borderColor = copy(C.line, 0)
    self.homeSearch.onTextChange = function() self.libScroll = 0 end
    pcall(function() self.homeSearch.javaObject:setPlaceholderText("Search") end)
    self.homeSearch:setVisible(false)
    self:addChild(self.homeSearch)

    -- Back and the game's own screen at the top right, where the app's window has Minimise and Close
    local back, vanilla = getText("UI_btn_back"), T("vanilla", "Game's mod screen")
    local ty = math.floor((ROW1 - BAR) / 2)
    self.back = button(self, self.width - PAD - (width(back) + 28), ty, back, nil, PZMMMods.onBack)
    self.vanilla = button(self, self.back:getX() - 8 - (width(vanilla) + 28), ty, vanilla, "quiet", PZMMMods.onVanilla)
    local y = self.height - BOTTOM + (BOTTOM - BAR) / 2
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
    self.modVer = (self.modVer or 0) + 1
    -- the load order, as the game will read it, and its problems (the Problems tab counts them)
    self.place, self.order = {}, {}
    pcall(function()
        local list = self.model:getActiveMods():getMods()
        for i = 0, list:size() - 1 do
            local id = list:get(i)
            self.place[id] = i + 1
            self.order[#self.order + 1] = id
        end
    end)
    self.oprob, self.oprobN, self.ofixable = self:orderProblems()
    for k in pairs(K.PROB) do K.PROB[k] = nil end
    for id in pairs(self.oprob) do K.PROB[id] = true end
    -- enabled mods that aren't installed get a row of their own
    local all = {}
    for _, d in pairs(self.model.mods) do all[#all + 1] = d end
    self.nghost = 0
    for _, id in ipairs(self.order) do
        if not self.model.mods[id] then
            all[#all + 1] = self:ghostOf(id)
            self.nghost = self.nghost + 1
        end
    end
    local counts = { All = 0, Enabled = 0, Disabled = 0, Problems = 0 }
    self.needed = {}
    for _, d in ipairs(all) do
        counts.All = counts.All + 1
        for name, f in pairs(SHOWN) do if f(d) then counts[name] = counts[name] + 1 end end
        if d.isActive and d.requireMods then
            for id, _ in pairs(d.requireMods) do self.needed[id] = (self.needed[id] or 0) + 1 end
        end
        if d.author == nil then
            d.author = ""
            pcall(function() d.author = tostring(d.modInfo:getAuthor() or "") end)
        end
        local match = q == "" or d.name:lower():find(q, 1, true) or d.modId:lower():find(q, 1, true) or d.author:lower():find(q, 1, true)
        local hid = self.hidden and self.hidden[d.modId] or false
        local shown = (self.filterHidden and hid) or (not self.filterHidden and not hid)
        if match and shown and (not keep or keep(d)) and (not self.favOnly or d.favorite) and (not self.filterKind or K.hasKind(self, d, self.filterKind)) and (not self.langOnly or d.hasLang) and (not self.filterTag or self:hasTag(d, self.filterTag)) then
            local name = lib and (lib.of[d.modId] or "Other") or K.SOURCES[source(d)]
            table.insert(byGroup[section(name)], d)
        end
    end
    self.counts = counts
    self.entries = {}
    local pos = lib and lib.pos or {}
    for g, name in ipairs(names) do
        local mods = byGroup[g]
        if #mods > 0 or (lib and lib.header[name] and q == "" and self.tab == "All") then
            -- the library's order, then by name for mods it doesn't list yet; or the Sort menu's, within each section
            local by = self.sortBy or "order"
            local function bad(m) return SHOWN.Problems(m) and 0 or 1 end
            -- Steam's size and update time per Workshop item (-1: local mods, last)
            if by == "size" or by == "changed" then
                for _, m in ipairs(mods) do
                    local st = self:steamOf(m)
                    m.sSize, m.sChanged = st and st.size or -1, st and st.updated or -1
                end
            end
            local favFirst = SETTINGS.favfirst and self:filtering()
            table.sort(mods, function(a, b)
                if favFirst and (a.favorite and true or false) ~= (b.favorite and true or false) then return a.favorite and true or false end
                if by == "size" and a.sSize ~= b.sSize then return a.sSize > b.sSize end
                if by == "changed" and a.sChanged ~= b.sChanged then return a.sChanged > b.sChanged end
                if by == "installed" then
                    local ia, ib = self.installed and self.installed[a.modId] or 0, self.installed and self.installed[b.modId] or 0
                    if ia ~= ib then return ia > ib end
                end
                if by == "enabled" and a.isActive ~= b.isActive then return a.isActive end
                if by == "status" and bad(a) ~= bad(b) then return bad(a) < bad(b) end
                if by == "order" then
                    local pa, pb = pos[a.modId] or 1e9, pos[b.modId] or 1e9
                    if pa ~= pb then return pa < pb end
                end
                return byName(a, b)
            end)
            local on = 0
            for _, d in ipairs(mods) do if d.isActive then on = on + 1 end end
            table.insert(self.entries, { head = name, on = on, n = #mods, first = mods[1] and mods[1].modId })
            if not self.collapsed[name] then
                for i, d in ipairs(mods) do table.insert(self.entries, { mod = d, last = i == #mods, g = name, nextId = mods[i + 1] and mods[i + 1].modId }) end
            end
        end
    end
    self.findings = nil
end

--- A mod that's enabled but not installed, as a row: a stand-in with just enough of a mod's fields.
function PZMMMods:ghostOf(id)
    self.ghosts = self.ghosts or {}
    local g = self.ghosts[id]
    if not g then
        g = { ghost = true, modId = id, name = id, isActive = true, isAvailable = true, isIncompatible = false, favorite = false,
            requireMods = {}, incompatibleWith = {}, category = "", modInfo = K.NOINFO }
        self.ghosts[id] = g
    end
    return g
end

--- Its row: Not installed, with its checkbox (Disable takes it out of the active list).
function PZMMMods:drawGhostRow(e, x, y, w, mx, my)
    local d = e.mod
    round(self, x, y, w, ROW, 8, C.panel, nil, false, e.last)
    if not e.last then rect(self, x, y + ROW - 1, w, 1, C.line, 0.6) end
    local over = mx >= x and mx < x + w and my >= y and my < y + ROW
    local h = ROW - (e.last and 8 or 0)
    local picked = self.selected == d
    local lit = picked or (over and not self.drag)
    if lit then
        local t, c, a = picked and 2 or 1, picked and C.onSelected or C.accent, picked and 1 or 0.7
        rect(self, x, y, w, h, C.accent, picked and 0.32 or 0.34)
        rect(self, x, y, w, t, c, a)
        rect(self, x, y + h - t, w, t, c, a)
        rect(self, x, y, t, h, c, a)
        rect(self, x + w - t, y, t, h, c, a)
        if K.SEL_MARK then self:drawTextureScaled(K.SEL_MARK, x - 5, y + math.floor(ROW / 2) - 7, 10, 14, 1, 1, 1, 1) end
    end
    local bx, by = x + 32, y + (ROW - BOX) / 2
    checkbox(self, bx, by, true, true)
    local mid = y + (ROW - H_SMALL) / 2
    local n = self.place[d.modId]
    if n then text(self, tostring(n), x + 84 - width(tostring(n)), mid, lit and C.onSelected or C.accentText) end
    local label = T("not_installed", "Not installed")
    local right = x + w - 16
    local gc = lit and C.onSelected or C.danger
    if TB.alert then
        self:drawTextureScaled(TB.alert, right - width(label) - 18, y + math.floor((ROW - 14) / 2), 14, 14, 1, gc.r, gc.g, gc.b)
        right = right - 18
    end
    bold(self, label, right - width(label), mid, gc)
    local tx = x + 98
    bold(self, fit(d.name, right - width(label) - 16 - tx), tx, mid, lit and C.onSelected or C.muted)
    if mx >= bx - 6 and mx < bx + BOX + 6 and my >= by - 6 and my < by + BOX + 6 then
        tip(self, "Disable\nIt isn't installed: this takes it out of the active list.")
    elseif over then
        tip(self, d.name .. "\nEnabled, but not installed: the game skips it. Install it, subscribe to it in Steam, or disable it.")
    end
    self.hits[#self.hits + 1] = { kind = "box", mod = d, x = bx - 6, y = by - 6, w = BOX + 12, h = BOX + 12 }
    self.hits[#self.hits + 1] = { kind = "row", mod = d, x = x, y = y, w = w, h = ROW }
end

-- --- the layout: header, toolbar, the list (left), the load order or a mod's details (right), footer

K.MODS_NOTICE = T("notice_mods", "installing from a zip, folder, GitHub or a link, removing a mod and rolling one back, change notes in a mod's details, the game's error log, profiles (new, rename, copy, delete), snapshots and pinning a mod's place, importing a profile (a share code, a Workshop collection or another manager's list), removing unused server profiles and setting up Project Viewpoint, download history, reinstall and cancel, unsubscribing on the Workshop page, subscriber count, Mod ID matches and listed by the author in details, the launch pre-flight check (fix and play, launch anyway, turn off those mods, trust files, don't ask again), the Check setup findings the game doesn't raise (the code risk scan with its risk tier and trust files, a Workshop item that is gone, banned or hidden, Lua syntax errors, duplicate copies and mod.info problems; server config findings: mods missing from or extra in a server's ini, build and requirement mismatches; game and launcher checks: the game's jar and classes, the launch flag, Java options, ZombieBuddy changed or broken, folders that don't exist; and map folder and overlap findings) and the desktop-only Check setup fixes (switch build, find a B42 version, verify game files, remove the extra copy, unsubscribe the others, get the original, update the game).")
function PZMMMods:listBox()
    local w = self.width - PAD * 2 - SIDE
    local y = TOP + FILTERS + self:modsNoticeH() + 12
    return PAD, y, w, self.height - BOTTOM - y
end
-- the right panel is flat and full height, flush with the screen's edge
function PZMMMods:sideBox()
    -- from the filters bar down, as the app's: the notice and banners sit over the list only
    local y = TOP + FILTERS
    return self.width - SIDE, y, SIDE, self.height - BOTTOM - y
end
local function entryHeight(e, i) return e.head and (HEAD + (i > 1 and GAP or 0)) or ROW end

function PZMMMods:prerender()
    local ok, err = pcall(self.prerenderScreen, self)
    if not ok and self.drawError ~= tostring(err) then
        self.drawError = tostring(err)
        print("PZMM - Client: drawing the screen failed: " .. self.drawError)
        self:say("Couldn't draw part of this screen. Game's mod screen, at the top right, still works.", "error")
    end
end

function PZMMMods:prerenderScreen()
    -- a button let go outside the screen sends no mouse-up: a drag still on would follow the mouse for good
    if self.drag or self.odrag or self.mapDrag or self.sbDrag or self.gridDrag then
        local ok, down = pcall(isMouseButtonDown, 0)
        if ok and not down then self.drag, self.odrag, self.mapDrag, self.sbDrag, self.gridDrag = nil, nil, nil, nil, nil end
    end
    rect(self, 0, 0, self.width, self.height, C.bg, 1)
    K.tint(self.search)
    K.tint(self.homeSearch)
    self.noticeHits = nil
    -- a new page reads its lists again (Home's servers, servers.txt)
    if self.page ~= self.lastPage then
        self.lastPage = self.page
        self.homeList, self.recent, self.srvCache, self.libLists, self.homeCheck = nil, nil, nil, nil, nil
        K.swapSearch(self)
    end
    -- words that came without a key press (a paste) filter too
    if self.search:getText() ~= self.searchSeen then self.search.onTextChange() end
    -- the blueprint grid, in the page body only (the filter bar stays flat): 24 px minors, 96 px majors
    local gy0 = self.page == "mods" and TOP + FILTERS or TOP
    local gy1 = self.height - BOTTOM
    self.bars = {}
    for x = 0, self.width, 24 do rect(self, x, gy0, 1, gy1 - gy0, C.grid, x % 96 == 0 and GRID.major or GRID.minor) end
    for y = gy0 - gy0 % 24 + 24, gy1 - 1, 24 do rect(self, 0, y, self.width, 1, C.grid, y % 96 == 0 and GRID.major or GRID.minor) end

    -- the app's top row: the skull, then the profile as a button
    rect(self, 0, 0, self.width, ROW1, C.panel, 1)
    rect(self, 0, ROW1 - 1, self.width, 1, C.line, 1)
    image(self, K.LOGO, PAD, (ROW1 - 44) / 2, 44, 44)
    local px = PAD + 44 + 14
    local py = (ROW1 - BAR) / 2
    local ty = py + (BAR - H_SMALL) / 2
    self.profileBox = nil
    local pend = px
    if self.lib and self.lib.profile then
        local name = self.lib.profile
        local w = 12 + width("Profile:") + 6 + width(name) + 1 + 12 + 12 + 10
        local mxp, myp = self:getMouseX(), self:getMouseY()
        local over = mxp >= px and mxp < px + w and myp >= py and myp < py + BAR
        local hot = over and self.app ~= nil
        round(self, px, py, w, BAR, 6, hot and C.hover or C.raised)
        outline(self, px, py, w, BAR, 6, hot and C.accent or C.line, hot and 0.7 or 1)
        text(self, "Profile:", px + 12, ty, C.muted)
        bold(self, name, px + 12 + width("Profile:") + 6, ty, hot and C.onSelected or C.text)
        if TB.chev_down then self:drawTextureScaled(TB.chev_down, px + w - 12 - 12, py + math.floor((BAR - 12) / 2), 12, 12, 1, C.muted.r, C.muted.g, C.muted.b) end
        self.profileBox = self.app and { x = px, y = py, w = w, h = BAR } or nil
        pend = px + w
        if over then
            tip(self, self.app and "Your PZMM profile\nIts sections sort My mods. Click to switch profile: PZMM asks first." or "Your sections\nKept in the game: drag mods between them, right-click a section to add, rename or remove one.")
        end
    else
        local msg = self.app and "Open PZMM to see your profile's sections here" or "Get the PZMM app: github.com/SEVNC3/pz-mod-manager/releases/latest"
        local mxp, myp = self:getMouseX(), self:getMouseY()
        local over = mxp >= px and mxp < px + width(msg) and myp >= py and myp < py + BAR
        text(self, msg, px, ty, over and C.accent or C.muted)
        -- with the app installed but no library yet, a click still lists its profiles; without it, opens the download
        self.profileBox = { x = px, y = py, w = width(msg), h = BAR }
        pend = px + width(msg)
    end
    -- the app's top bar actions the game can do too, at the right: icon and label, explained on hover; the
    -- labels drop when the profile and the buttons don't both fit before Back
    local shown = {}
    for _, a in ipairs(ACTIONS) do if not a.app or self.app then shown[#shown + 1] = a end end
    local full = -4
    for _, a in ipairs(shown) do full = full + width(a[4]) + 46 + 4 end
    local labelled = self.vanilla:getX() - 16 - full >= pend + 16
    local ax = self.vanilla:getX() - 16 - (labelled and full or #shown * (BAR + 4) - 4)
    local mxa, mya = self:getMouseX(), self:getMouseY()
    self.actionBoxes = {}
    for _, a in ipairs(shown) do
        local aw, over = toolButton(self, a[2], labelled and a[4] or nil, ax, py, mxa, mya)
        if over then tip(self, a[3]) end
        self.actionBoxes[#self.actionBoxes + 1] = { a[1], ax, py, aw }
        ax = ax + aw + 4
    end

    -- its tab bar: My mods, the active tab (soft red fill, light edges, white underline)
    rect(self, 0, ROW1, self.width, ROW2, C.panel, 1)
    rect(self, 0, TOP - 1, self.width, 1, C.line, 1)
    local tx = 12
    local mx0, my0 = self:getMouseX(), self:getMouseY()
    self.pageBoxes = {}
    for _, pg in ipairs(PAGES) do
        if pg.hidden then
            -- (a page kept out of the bar)
        else
        local feed = self.feed or self:liteFeed()
        -- What's new: the updates you haven't seen there yet
        local n = 0
        if pg[1] == "feed" and feed then
            for _, e in ipairs(feed.entries) do if e.updated > (self.feedSeen or 0) then n = n + 1 end end
        end
        local badge = n > 0 and tostring(n) or nil
        -- Java mods: a red dot while enabled mods need ZombieBuddy (worked out twice a second)
        local alert = false
        if pg[1] == "java" then
            local t = getTimestampMs()
            if not self.zbAt or t - self.zbAt > 500 then self.zbAt, self.zbN = t, self:zbNeeded() end
            alert = (self.zbN or 0) > 0
        end
        local tw = width(pg[2]) + 28 + (badge and math.max(18, width(badge) + 10) + 6 or 0) + (alert and 14 or 0)
        local on = self.page == pg[1]
        local over = mx0 >= tx and mx0 < tx + tw and my0 >= ROW1 and my0 < TOP
        if over then tip(self, pg[3]) end
        if on or over then
            round(self, tx, ROW1, tw, ROW2 - 1, 4, C.accent, 0.32, true, false)
            rect(self, tx + 4, ROW1, tw - 8, 1, C.accent, 0.7)
            rect(self, tx, ROW1 + 4, 1, ROW2 - 5, C.accent, 0.7)
            rect(self, tx + tw - 1, ROW1 + 4, 1, ROW2 - 5, C.accent, 0.7)
            rect(self, tx, TOP - 3, tw, 2, C.onSelected, 1)
        end
        local ty = ROW1 + (ROW2 - H_SMALL) / 2
        text(self, pg[2], tx + 14, ty, (on or over) and C.onSelected or C.muted)
        if alert then round(self, tx + 14 + width(pg[2]) + 6, ROW1 + math.floor((ROW2 - 8) / 2), 8, 8, 4, C.danger) end
        if badge then
            local bxp = tx + 14 + width(pg[2]) + 6 + (alert and 14 or 0)
            local bw = math.max(18, width(badge) + 10)
            round(self, bxp, ROW1 + math.floor((ROW2 - 18) / 2), bw, 18, 9, C.accent)
            text(self, badge, bxp + math.floor((bw - width(badge)) / 2), ty, C.onAccent)
        end
        self.pageBoxes[#self.pageBoxes + 1] = { pg[1], tx, tw }
        tx = tx + tw + 2
        end
    end
    local lx = self.width - PAD - #LINKS * 36 + 4
    local ly = ROW1 + (ROW2 - BAR) / 2
    local mx0, my0 = self:getMouseX(), self:getMouseY()
    self.linkBoxes = {}
    for _, l in ipairs(LINKS) do
        local over = mx0 >= lx and mx0 < lx + BAR and my0 >= ly and my0 < ly + BAR
        if over then
            tip(self, l[4])
            round(self, lx, ly, BAR, BAR, 6, C.hover)
            outline(self, lx, ly, BAR, BAR, 6, C.accent, 0.7)
        end
        if l[1] then self:drawTextureScaled(l[1], lx + 6, ly + math.floor((BAR - 20) / 2), 20, 20, over and 1 or 0.85, 1, 1, 1) end
        self.linkBoxes[#self.linkBoxes + 1] = { l[2], lx, ly }
        lx = lx + 36
    end

    self.search:setVisible(self.page == "mods")
    if self.page == "mods" then
        self.search:setX(PAD + 8)
        self.search:setY(TOP + 12) -- (its text sits 2 px high in the box: this centres it in the frame)
        self.search:setWidth(168)
    end
    if self.page ~= "home" then self.homeSearch:setVisible(false) end
    if self.page == "feed" then
        self:drawFeed()
    elseif self.page ~= "mods" then
        self:drawPage()
    else
    -- the filters bar: search, then All / Enabled / Disabled / Problems as one segmented control
    rect(self, 0, TOP + FILTERS - 1, self.width, 1, C.line, 1)
    local y = TOP + 8
    round(self, PAD, y, 180, BAR, 6, C.raised)
    local focused = false
    pcall(function() focused = self.search:isFocused() end)
    if focused then
        -- a 2 px white ring while typing
        outline(self, PAD, y, 180, BAR, 6, C.focus)
        outline(self, PAD - 1, y - 1, 182, BAR + 2, 6, C.focus)
    else
        outline(self, PAD, y, 180, BAR, 6, C.line)
    end
    local x = PAD + 180 + 8
    self.tabBoxes = {}
    local labels, segW = {}, 0
    for i, t in ipairs(TABS) do
        local n = self.counts and self.counts[t] or 0
        labels[i] = (t == "Problems" and n > 0) and (K.TAB_NAMES[t] .. " (" .. n .. ")") or K.TAB_NAMES[t]
        segW = segW + width(labels[i]) + 24
    end
    local mx, my = self:getMouseX(), self:getMouseY()
    for i, t in ipairs(TABS) do
        local w = width(labels[i]) + 24
        local on = self.tab == t
        if on then roundEnds(self, x, y, w, BAR, 6, C.accent, 0.32, i == 1, i == #TABS) end
        local over = mx >= x and mx < x + w and my >= y and my < y + BAR
        if over then tip(self, K.TAB_TIPS[t]) end
        local s = on and bold or text
        s(self, labels[i], x + 12, y + (BAR - H_SMALL) / 2, on and C.onSelected or over and C.text or C.muted)
        self.tabBoxes[#self.tabBoxes + 1] = { t, x, y, w, BAR }
        x = x + w
    end
    outline(self, PAD + 188, y, segW, BAR, 6, C.line)
    self:drawTools(PAD + 188 + segW + 8, y, mx, my)

    self:drawList()
    self:drawNotice(0, TOP + FILTERS, self.width, K.MODS_NOTICE, mx, my, true)
    self:drawBanners(TOP + FILTERS + self.noticeCache.h, mx, my)
    self:drawSide()
    end

    -- footer
    rect(self, 0, self.height - BOTTOM, self.width, BOTTOM, C.panel, 1)
    rect(self, 0, self.height - BOTTOM, self.width, 1, C.line, 1)
    local changed = self:changes()
    local right = self.accept:getX() - 16
    if changed > 0 then
        local s = changed .. " change" .. (changed == 1 and "" or "s") .. " not saved yet"
        right = right - width(s)
        text(self, s, right, self.height - BOTTOM + (BOTTOM - H_SMALL) / 2, C.muted)
        right = right - 16
    end
    self:drawActivityBar(right)
    if self.back:isMouseOver() then tip(self, "Back\nTo the main menu. Changes not saved yet: you're asked first.") end
    if self.vanilla:isMouseOver() then tip(self, "Game's mod screen\nThe game's own mod screen, for presets and load order.") end
    if self.accept:isMouseOver() then tip(self, "Accept\nSave your mods. The game restarts its Lua when the mod set needs it.") end
end

--- 0-2: whether the load order of the mods on both then and now, and the map order, differ from when the
--- screen opened (modsOrig is a copy of the ActiveMods taken then, map order included). Worked out at most
--- every 250 ms: the footer asks every frame.
function PZMMMods:orderDiffs(fresh)
    local t = getTimestampMs()
    if not fresh and self.diffAt and t - self.diffAt < 250 then return self.diffN end
    local n = 0
    pcall(function()
        local o = self.modsOrig
        if not o then return end
        local function list(l)
            local out = {}
            for i = 0, l:size() - 1 do out[#out + 1] = l:get(i) end
            return out
        end
        local cur, was = list(self.model:getActiveMods():getMods()), list(o:getMods())
        local inCur, inWas = {}, {}
        for _, id in ipairs(cur) do inCur[id] = true end
        for _, id in ipairs(was) do inWas[id] = true end
        -- mods turned on or off are counted on their own: only the order of those on both times
        local a, b = {}, {}
        for _, id in ipairs(cur) do if inWas[id] then a[#a + 1] = id end end
        for _, id in ipairs(was) do if inCur[id] then b[#b + 1] = id end end
        if table.concat(a, "\n") ~= table.concat(b, "\n") then n = n + 1 end
        if table.concat(list(self.model:getActiveMods():getMapOrder()), "\n") ~= table.concat(list(o:getMapOrder()), "\n") then n = n + 1 end
    end)
    self.diffAt, self.diffN = t, n
    return n
end

--- Changes not saved yet: mods turned on or off, favourites, the load and map order.
function PZMMMods:changes(fresh)
    local n = self:orderDiffs(fresh)
    for _, d in pairs(self.model.mods) do
        if d.isActive ~= d.defaultActive then n = n + 1 end
        -- the game keeps favourites as strings or booleans: compare whether there is one
        if (d.favorite and true or false) ~= (d.defaultFav and true or false) then n = n + 1 end
    end
    return n
end

function PZMMMods:render()
    ISPanelJoypad.render(self)
    self:launchIfNeeded()
    self:settle()
    self:afterAcks()
    self:appUp()
    if self.app then self:pollAppLog() end
    self:drawActivity()
    drawTip(self)
end
--- Say what just happened: a line in the activity, which the bar at the bottom shows. Lines starting
--- "Couldn't" or "Can't" are errors.
function PZMMMods:say(msg, kind)
    msg = tostring(msg or ""):gsub("\n", ": ")
    if msg == "" then return end
    if type(kind) ~= "string" then kind = nil end
    if not kind then kind = (msg:match("^Couldn't") or msg:match("^Can't")) and "error" or "info" end
    self:addLog(msg, kind, "game")
end

-- the activity, kept while the game runs: { time, kind, text, from = "game" | "app" | "sync" }
PZMMMods.log = PZMMMods.log or {}
K.LOG_MAX = 300

--- Add a line. The game's own (from "game") also go to activity_game.txt, which PZMM shows in its activity;
--- what the game noticed PZMM do ("sync") stays here, as PZMM has its own line for it.
function PZMMMods:addLog(text, kind, from)
    local log = PZMMMods.log
    log[#log + 1] = { time = now(), kind = kind or "info", text = text, from = from or "game" }
    while #log > K.LOG_MAX do table.remove(log, 1) end
    if from == "game" then
        local lines = {}
        for _, e in ipairs(log) do
            if e.from == "game" then lines[#lines + 1] = e.time .. "\t" .. e.kind .. "\t" .. e.text:gsub("[\t\r\n]", " ") end
        end
        while #lines > 200 do table.remove(lines, 1) end
        pcall(function()
            local w = getFileWriter("pzmm/activity_game.txt", true, false)
            w:write(table.concat(lines, "\n") .. "\n")
            w:close()
        end)
    end
end

--- PZMM's lines that concern the game (activity_app.txt), new ones since the last look, every two seconds.
function PZMMMods:pollAppLog()
    local t = getTimestampMs()
    if self.appLogAt and t - self.appLogAt < 2000 then return end
    self.appLogAt = t
    -- new means after the last line shown: by its second, then its place within that second
    local first = PZMMMods.appSeen == nil
    local seen, seenN = PZMMMods.appSeen or 0, PZMMMods.appSeenN or 0
    local sec, n, added = nil, 0, false
    for _, e in ipairs(readActivity("activity_app.txt")) do
        if e.time == sec then n = n + 1 else sec, n = e.time, 1 end
        if e.time > seen or (e.time == seen and n > seenN) then
            self:addLog(e.text, e.kind, "app")
            PZMMMods.log[#PZMMMods.log].time = e.time
            seen, seenN, added = e.time, n, true
        end
    end
    PZMMMods.appSeen, PZMMMods.appSeenN = seen, seenN
    -- the first look loads what PZMM had already said as history: old errors don't light the red dot
    if first then PZMMMods.logSeen = now() end
    -- its lines carry their own time: keep the whole log in time order (ties stay in the order they came)
    if added then
        local log = PZMMMods.log
        for i, e in ipairs(log) do e.ord = i end
        table.sort(log, function(a, b)
            if a.time ~= b.time then return a.time < b.time end
            return a.ord < b.ord
        end)
    end
    if settingsChanged then
        settingsChanged = false
        self:addLog("Settings changed this screen's options.", "info", "sync")
    end
end

local function clock(t)
    local ok, out = pcall(function() return os.date("%H:%M", K.LT(t)) end)
    return ok and tostring(out) or ""
end

--- The activity bar at the bottom, as the app's: the toggle (with a red dot for an error you haven't seen),
--- the newest line, and Copy. Drawn in the footer, left of what isn't saved yet and Accept.
function PZMMMods:drawActivityBar(right)
    local mx, my = self:getMouseX(), self:getMouseY()
    local fy = self.height - BOTTOM
    local by = fy + math.floor((BOTTOM - BAR) / 2)
    local log = PZMMMods.log
    local last = log[#log]
    if self.logOpen and last then PZMMMods.logSeen = last.time end
    local unseen = false
    for i = #log, 1, -1 do
        if log[i].time <= (PZMMMods.logSeen or 0) then break end
        if log[i].kind == "error" then unseen = true end
    end
    -- the toggle
    local label = T("act_title", "Activity")
    local tw = 16 + 8 + 8 + 8 + width(label) + 14
    local tx = PAD - 4
    local over = mx >= tx and mx < tx + tw and my >= by and my < by + BAR
    if over then
        round(self, tx, by, tw, BAR, 6, C.hover)
        outline(self, tx, by, tw, BAR, 6, C.accent, 0.7)
    end
    local chev = self.logOpen and TB.chev_down or TB.chev_up
    if chev then self:drawTextureScaled(chev, tx + 10, by + math.floor((BAR - 16) / 2), 16, 16, 1, C.text.r, C.text.g, C.text.b) end
    local dc = unseen and C.danger or C.muted
    local da = unseen and 1 or 0.6
    -- busy: PZMM hasn't answered a request yet, so the dot pulses
    if self:waiting() then dc, da = C.accentText, 0.4 + 0.6 * (math.sin(getTimestampMs() / 250) + 1) / 2 end
    round(self, tx + 34, by + math.floor((BAR - 8) / 2), 8, 8, 3, dc, da)
    bold(self, label, tx + 50, by + (BAR - H_SMALL) / 2, C.text)
    if over then tip(self, "Activity\nWhat you did here, and what PZMM did that reaches the game. The app also has the download history and Cancel.") end
    self.logToggle = { x = tx, y = by, w = tw, h = BAR }
    -- Copy, at the right of the bar
    local cx = right - BAR
    local overC = #log > 0 and mx >= cx and mx < cx + BAR and my >= by and my < by + BAR
    if overC then
        round(self, cx, by, BAR, BAR, 6, C.hover)
        outline(self, cx, by, BAR, BAR, 6, C.accent, 0.7)
        tip(self, "Copy\nThe activity, as text.")
    end
    if TB.copy then self:drawTextureScaled(TB.copy, cx + math.floor((BAR - 16) / 2), by + math.floor((BAR - 16) / 2), 16, 16, #log > 0 and 1 or 0.35, C.text.r, C.text.g, C.text.b) end
    self.logCopy = { x = cx, y = by, w = BAR, h = BAR }
    -- while PZMM hasn't answered: what it waits for, and a bar with a segment sweeping along it
    local lx = tx + tw + 12
    if self:waiting() then
        local msg = T("act_waiting", "Waiting for PZMM...")
        text(self, msg, lx, by + (BAR - H_SMALL) / 2, C.muted)
        local bx2 = lx + width(msg) + 12
        if bx2 + 120 <= cx - 12 then
            local by2 = by + math.floor((BAR - 6) / 2)
            round(self, bx2, by2, 120, 6, 3, C.line)
            local sx = bx2 - 40 + math.floor(160 * ((getTimestampMs() % 1400) / 1400))
            local a, b = math.max(bx2, sx), math.min(bx2 + 120, sx + 40)
            if b > a then rect(self, a, by2, b - a, 6, C.accentText, 1) end
        end
    elseif last then
        -- the newest line
        local str = (last.from == "app" and "PZMM: " or "") .. last.text
        -- an error from over 10 minutes ago is history, not how things are now: muted, with its age
        local old = last.kind == "error" and now() - (tonumber(last.time) or 0) > 600
        if old then str = K.ago(last.time) .. ": " .. str end
        text(self, fit(str, cx - 12 - lx), lx, by + (BAR - H_SMALL) / 2, last.kind == "error" and not old and C.danger or C.muted)
    end
end

-- the activity's kind filters, as the app's: All, then each kind of line
K.LOG_KINDS = { "all", "install", "update", "remove", "error", "info" }
K.LOG_NAMES = { all = T("act_kind_all", "All"), install = T("act_kind_install", "Installs"), update = T("act_kind_update", "Updates"),
    remove = T("act_kind_remove", "Removals"), error = T("act_kind_error", "Errors"), info = T("act_kind_info", "Other") }
--- Whether a line of kind `k` shows under filter `f` (Other is any kind that has no chip of its own).
function K.logShows(f, k)
    if f == "all" then return true end
    if f == "info" then return k ~= "install" and k ~= "update" and k ~= "remove" and k ~= "error" end
    return k == f
end

--- The open activity: a bar with the kind filters and Expand, then every line with its time, repeats as one
--- with a count, newest at the bottom. It scrolls (the wheel), and stays at the newest unless you scrolled up.
--- Expanded, it fills the screen above the footer.
function PZMMMods:drawActivity()
    if not self.logOpen then return end
    local mx, my = self:getMouseX(), self:getMouseY()
    local head = BAR + 16
    local ph = self.logExpanded and (self.height - BOTTOM - ROW1) or math.min(214 + head, math.floor(self.height * 0.4) + head)
    local px, py, pw = 0, self.height - BOTTOM - ph, self.width
    self.logBox = { x = px, y = py, w = pw, h = ph, view = ph - head }
    self.logHits = {}
    rect(self, px, py, pw, ph, C.panel, 1)
    rect(self, px, py, pw, 1, C.line, 1)
    -- the bar: kind chips as one segmented control, Expand at the right
    local f = self.logKind or "all"
    local ty = py + 8
    local segW = 0
    for _, k in ipairs(K.LOG_KINDS) do segW = segW + width(K.LOG_NAMES[k]) + 24 end
    local tx = PAD
    for i, k in ipairs(K.LOG_KINDS) do
        local label = K.LOG_NAMES[k]
        local tw = width(label) + 24
        local on = f == k
        if on then roundEnds(self, tx, ty, tw, BAR, 6, C.accent, 0.32, i == 1, i == #K.LOG_KINDS) end
        local over = mx >= tx and mx < tx + tw and my >= ty and my < ty + BAR
        local fn = on and bold or text
        fn(self, label, tx + 12, ty + (BAR - H_SMALL) / 2, on and C.onSelected or over and C.text or C.muted)
        local kk = k
        self.logHits[#self.logHits + 1] = { x = tx, y = ty, w = tw, h = BAR, fn = function() self.logKind, self.logScroll = kk, 0 end }
        tx = tx + tw
    end
    outline(self, PAD, ty, segW, BAR, 6, C.line)
    local ex = pw - PAD - BAR
    local _, over = toolButton(self, self.logExpanded and TB.minimize or TB.maximize, nil, ex, ty, mx, my)
    if over then tip(self, self.logExpanded and "Collapse\nBack to the short list." or "Expand activity\nThe list at full height.") end
    self.logHits[#self.logHits + 1] = { x = ex, y = ty, w = BAR, h = BAR, fn = function() self.logExpanded, self.logScroll = not self.logExpanded, 0 end }
    rect(self, px, py + head - 1, pw, 1, C.line, 0.6)
    local rows = {}
    for _, e in ipairs(PZMMMods.log) do
        if K.logShows(f, e.kind) then
            local str = (e.from == "app" and "PZMM: " or "") .. e.text
            local prev = rows[#rows]
            if prev and prev.text == str and prev.kind == e.kind then
                prev.times, prev.time = prev.times + 1, e.time
            else
                rows[#rows + 1] = { time = e.time, kind = e.kind, text = str, times = 1 }
            end
        end
    end
    local view = ph - head
    local rh = H_SMALL + 8
    if #rows == 0 then
        local m = #PZMMMods.log == 0 and "Nothing yet: what you do here, and what PZMM does for the game, shows here." or "Nothing of this kind."
        text(self, m, px + math.floor((pw - width(m)) / 2), py + head + 24, C.muted)
        return
    end
    local full = #rows * rh + 16
    self.logFull = full
    self.logScroll = math.max(0, math.min(self.logScroll or 0, full - view))
    -- 0 is the newest at the bottom; scrolling up reads older lines
    local y = py + ph - 8 - full + 8 + self.logScroll
    self:setStencilRect(px, py + head, pw, view)
    local tw = width("00:00")
    for _, r in ipairs(rows) do
        if y + rh >= py + head and y <= py + ph then
            if r.kind == "error" then
                rect(self, PAD, y, pw - PAD * 2, rh, C.danger, 0.1)
                rect(self, PAD, y, 2, rh, C.danger, 1)
            end
            local c = r.kind == "error" and C.danger or C.text
            text(self, clock(r.time), PAD + 10, y + 4, C.muted)
            local times = r.times > 1 and ("x" .. r.times) or nil
            local room = pw - PAD * 2 - 10 - tw - 14 - (times and width(times) + 14 or 0) - 10
            text(self, fit(r.text, room), PAD + 10 + tw + 14, y + 4, c)
            if times then text(self, times, pw - PAD - 10 - width(times), y + 4, C.muted) end
        end
        y = y + rh
    end
    self:clearStencilRect()
end

--- The game's current language, as its Translate folder is named (EN, FR, PTBR...).
function PZMMMods:langName()
    if not self.lang then
        pcall(function() self.lang = tostring(Translator.getLanguage():name()) end)
        if not self.lang then pcall(function() self.lang = tostring(getCore():getOptionLanguageName()) end) end
        self.lang = self.lang or "EN"
    end
    return self.lang
end

--- In my language: which mods ship media/lua/shared/Translate/<LANG>, probed 25 a frame while the filter is on.
function PZMMMods:scanLang()
    local lang = self:langName()
    local n = 0
    for _, d in pairs(self.model.mods) do
        if d.langOf ~= lang then
            d.langOf, d.hasLang = lang, false
            pcall(function()
                local a = listFilesInModDirectory(d.modId, "media/lua/shared/Translate/" .. lang)
                d.hasLang = a ~= nil and a:size() > 0
            end)
            n = n + 1
            if n >= 25 then break end
        end
    end
    if n < 25 then self.langDone = true end
    -- the list is rebuilt when the scan finishes, and while it runs at most every 500 ms
    local t = getTimestampMs()
    if n > 0 and (self.langDone or not self.langViewAt or t - self.langViewAt >= 500) then
        self.langViewAt = t
        self:updateView()
    end
end

--- The languages a mod has texts in (Translate/<LANG> folders the game knows), probed once per mod: "FR, DE" or "".
K.LANGS = { "AR", "CA", "CH", "CN", "CS", "DA", "DE", "EN", "ES", "ES_CL", "ES_MX", "FI", "FR", "HU", "ID", "IT", "JP", "KO", "NL", "NO", "PL", "PT", "PTBR", "RO", "RU", "TH", "TR", "UA" }
function PZMMMods:langsOf(d)
    if d.langList == nil then
        local out = {}
        for _, l in ipairs(K.LANGS) do
            pcall(function()
                local a = listFilesInModDirectory(d.modId, "media/lua/shared/Translate/" .. l)
                if a ~= nil and a:size() > 0 then out[#out + 1] = l end
            end)
        end
        d.langList = table.concat(out, ", ")
    end
    return d.langList
end

function PZMMMods:drawList()
    if self.langOnly and not self.langDone then self:scanLang() end
    self.ovOf = self:overlaps()
    local lx, ly, lw, lh = self:listBox()
    self:setStencilRect(lx - 6, ly, lw + 6, lh)
    local mx, my = self:getMouseX(), self:getMouseY()
    -- rows scrolled out of sight aren't hovered (a drag still follows the mouse)
    local dmy = my
    if mx < lx or mx >= lx + lw or my < ly or my >= ly + lh then my = -1e9 end
    local y = ly - self.scroll
    local w = lw - 14
    self.hits = {}
    for i, e in ipairs(self.entries) do
        local h = entryHeight(e, i)
        if y + h >= ly and y <= ly + lh then
            if e.head then
                local top = y + (i > 1 and GAP or 0)
                round(self, lx, top, w, HEAD, 8, C.panel, nil, true, false)
                handle(self, lx + 12, top + HEAD / 2 - 6)
                local chev = self.collapsed[e.head] and K.CHEV_RIGHT or K.CHEV_DOWN
                if chev then
                    self:drawTextureScaled(chev, lx + 28, top + math.floor(HEAD / 2) - 7, 14, 14, 1, C.muted.r, C.muted.g, C.muted.b)
                else
                    text(self, self.collapsed[e.head] and ">" or "v", lx + 30, top + (HEAD - H_SMALL) / 2, C.muted)
                end
                local right = tape(self, e.head, lx + 48, top + HEAD / 2)
                text(self, e.on .. " of " .. e.n .. " enabled", right + 12, top + (HEAD - H_SMALL) / 2, C.muted)
                if self.collapsed[e.head] then
                    round(self, lx, top + HEAD - 8, w, 8, 8, C.panel, nil, false, true)
                end
                -- a section the library lists can be dragged by its handle, as in the app
                local movable = not self.lib or self.lib.header[e.head]
                if movable and mx >= lx + 6 and mx < lx + 26 and my >= top and my < top + HEAD and not self.drag then
                    tip(self, "Drag to sort\nMove the whole section, its mods with it.")
                elseif mx >= lx and mx < lx + w - 40 and my >= top and my < top + HEAD and not self.drag then
                    tip(self, e.head .. "\nClick to fold or unfold. Right-click to add, rename or remove sections.")
                end
                if movable then self.hits[#self.hits + 1] = { kind = "dragsec", g = e.head, x = lx + 6, y = top, w = 20, h = HEAD } end
                -- a section of the profile's can be removed from its right end (on hover), as in the app
                if movable and e.head ~= "Other" and mx >= lx and mx < lx + w and my >= top and my < top + HEAD and not self.drag then
                    local tx0 = lx + w - 40
                    local tover = mx >= tx0 and mx < tx0 + 28
                    local tc = tover and C.danger or C.muted
                    if TB.trash then self:drawTextureScaled(TB.trash, tx0 + 6, top + math.floor((HEAD - 16) / 2), 16, 16, 1, tc.r, tc.g, tc.b) end
                    if tover then tip(self, "Remove section\nIts mods stay: they go under Other.") end
                    local g = e.head
                    self.hits[#self.hits + 1] = { kind = "fn", fn = function() self:removeSection(g) end, x = tx0, y = top, w = 28, h = HEAD }
                end
                self.hits[#self.hits + 1] = { kind = "head", g = e.head, x = lx, y = top, w = w, h = HEAD, tx0 = lx + 48, tx1 = right }
            else
                self:drawRow(e, lx, y, w, mx, my)
            end
            -- what's being dragged stays in place, faded, as in the app
            local dr = self.drag
            if dr and dr.moved and ((dr.sec and (e.head or e.g) == dr.sec) or (dr.mod and e.mod == dr.mod)) then
                local top = (e.head and i > 1) and y + GAP or y
                rect(self, lx - 6, top, w + 6, y + h - top, C.bg, 0.6)
            end
        end
        y = y + h
    end
    if self.drag then self:drawDrag(lx, ly, w, lh, mx, dmy) end
    if #self.entries == 0 then
        local q = self.search and self.search:getText() or ""
        local title, hint
        if q ~= "" then
            title, hint = "No mods match \"" .. q .. "\"", "Try another word, or clear the search."
        elseif self.tab ~= "All" then
            title, hint = "No mods in " .. (K.TAB_NAMES[self.tab] or self.tab), "Try another tab."
        else
            title, hint = T("mods_empty_title", "Nothing in the stockpile yet."), T("mods_empty_hint", "Subscribe to mods in Steam: subscriptions show up here once you Rescan.")
        end
        -- a block 460 wide, a little way down: the title in the marker, its hint under it
        local bw = math.min(460, w)
        local bx = lx + math.floor((w - bw) / 2)
        local by = ly + math.floor(lh * 0.12)
        text(self, fit(title, bw, MARKER), bx + math.floor((bw - math.min(bw, width(title, MARKER))) / 2), by, C.text, MARKER)
        by = by + H_MARKER + 8
        for _, l in ipairs(wrap(hint, bw)) do
            text(self, l, bx + math.floor((bw - width(l)) / 2), by, C.muted)
            by = by + H_SMALL + 2
        end
    end
    self:clearStencilRect()
    self.listHeight = y + self.scroll - ly
    self.scroll = math.max(0, math.min(self.scroll, self.listHeight - lh))
    scrollbar(self, "scroll", lx, ly, lw, lh, self.listHeight)
end

--- A library line's section name ("Other" for a nameless break), or nil when it isn't a section's line.
local function sectionName(l)
    local n = l:match("^# %-%-%-%s*(.*)$")
    return n and (n ~= "" and n or "Other")
end
--- Where section `g` ends in `lines` (the next section's line, else past the end), and whether it's there.
local function sectionEnd(lines, g)
    local found = false
    for i, l in ipairs(lines) do
        local n = sectionName(l)
        if n then
            if found then return i, true end
            found = n == g
        end
    end
    return #lines + 1, found
end

--- A drag from a mod's or a section's handle: where it would land (a mod before a mod or at the end of a
--- section; a section between sections), drawn as the app's red feather, with a see-through copy of what's
--- dragged under the mouse. The list scrolls near its edges.
function PZMMMods:drawDrag(lx, ly, w, lh, mx, my)
    local dr = self.drag
    if not dr.moved then
        if math.abs(my - dr.y0) < 5 then return end
        dr.moved = true
    end
    if my < ly + 30 then self.scroll = math.max(0, self.scroll - 8) end
    if my > ly + lh - 30 then self.scroll = math.max(0, math.min(self.scroll + 8, (self.listHeight or 0) - lh)) end
    dr.target = nil
    -- off the list, a drop does nothing
    local onList = mx >= lx and mx < lx + w and my >= ly and my <= ly + lh
    local y = onList and ly - self.scroll or math.huge
    if dr.sec then
        -- before the section head nearest the mouse, or at the end; where it is already, nothing moves
        local best
        y = ly - self.scroll
        for i, e in ipairs(self.entries) do
            if e.head then
                local top = y + (i > 1 and GAP or 0)
                if not best or math.abs(my - top) < math.abs(my - best.y) then best = { before = e.head, y = top } end
            end
            y = y + entryHeight(e, i)
        end
        if not best or math.abs(my - y) < math.abs(my - best.y) then best = { y = y } end
        if onList and best.before ~= dr.sec and best.before ~= dr.next then dr.target = best end
        y = math.huge
    end
    for i, e in ipairs(self.entries) do
        local h = entryHeight(e, i)
        if my >= y and my < y + h then
            local g = e.head or e.g
            local known = not self.lib or g == "Other" or self.lib.header[g]
            if known and e.head then
                dr.target = { g = e.head, before = e.first, y = y + h }
            elseif known then
                local upper = my < y + h / 2
                dr.target = { g = e.g, before = upper and e.mod.modId or e.nextId, y = upper and y or y + h }
            end
            break
        end
        y = y + h
    end
    local t = dr.target
    if t and (dr.sec or t.before ~= dr.mod.modId) then
        if K.FEATHER then self:drawTextureScaled(K.FEATHER, lx, t.y, w, 20, 1, C.accent.r, C.accent.g, C.accent.b) end
        rect(self, lx, t.y, w, 1, C.dropEdge, 1)
    end
    local gw, half = math.min(w, 480), math.floor(ROW / 2)
    local gx, gy = mx - 16, my - half
    round(self, gx, gy, gw, ROW, 6, C.panel, 0.85)
    outline(self, gx, gy, gw, ROW, 6, C.accent, 0.7)
    handle(self, gx + 12, gy + half - 6)
    if dr.sec then
        tape(self, dr.sec, gx + 32, gy + half)
    else
        bold(self, fit(dr.mod.name, gw - 48), gx + 32, gy + math.floor((ROW - H_SMALL) / 2), C.text)
    end
end

--- Drop the dragged mod: the library's lines with it moved (written now, so the game shows it at once),
--- and a quiet request for PZMM to move it in the profile the same way.
function PZMMMods:placeMod(d, g, before)
    if not self.lib or before == d.modId then return end
    local all, lines, mine = self:libLines(), {}, nil
    for _, l in ipairs(all) do
        if l:match("^[+-](.+)$") == d.modId then mine = l else lines[#lines + 1] = l end
    end
    mine = mine or ((d.isActive and "+" or "-") .. d.modId)
    local function lineOf(id)
        for i, l in ipairs(lines) do if l:match("^[+-](.+)$") == id then return i end end
    end
    local at = before and lineOf(before)
    if not at then
        local found
        at, found = sectionEnd(lines, g)
        if not found then
            -- mods outside any section are "Other": a nameless break, as the app writes it
            if g ~= "Other" then return end
            lines[#lines + 1] = "# ---"
            at = #lines + 1
        end
        -- the mod to land before isn't listed yet (installed since): it joins the section's end first
        local b = before and self.model.mods[before]
        if b then
            table.insert(lines, at, (b.isActive and "+" or "-") .. before)
        end
    end
    table.insert(lines, at, mine)
    -- dropped where it already is: nothing to save, ask or undo
    if table.concat(lines, "\n") == table.concat(all, "\n") then return end
    self:remember()
    self.collapsed[g] = nil
    if not self:writeLibrary(lines) then return end
    self:toApp("place\t" .. d.modId .. "\t" .. g .. "\t" .. (before or ""), d.name .. " moved to " .. g .. ".")
end

--- Move section `g` (its line and its mods) before section `before`, else to the end, as the app does.
function PZMMMods:placeSection(g, before)
    local lines, from, to = self:libLines(), nil, nil
    for i, l in ipairs(lines) do
        local n = sectionName(l)
        if n and from and not to then to = i end
        if n == g and not from then from = i end
    end
    if not from then return end
    to = to or #lines + 1
    local block, rest = {}, {}
    for i, l in ipairs(lines) do
        if i >= from and i < to then block[#block + 1] = l else rest[#rest + 1] = l end
    end
    local at = #rest + 1
    for i, l in ipairs(rest) do
        if before and sectionName(l) == before then at = i; break end
    end
    for k, l in ipairs(block) do table.insert(rest, at + k - 1, l) end
    if table.concat(rest, "\n") == table.concat(lines, "\n") then return end
    self:remember()
    if self:writeLibrary(rest) then self:toApp("section\tmove\t" .. g .. "\t" .. (before or ""), g .. " moved.") end
end

--- A library's lines for a request, tab-separated, without its "# PZMM profile:" line.
local function libBody(lines)
    local out = {}
    for _, l in ipairs(lines) do if not l:match("^# PZMM profile:") then out[#out + 1] = l end end
    return table.concat(out, "\t")
end

local SORTS = { { "order", T("sort_library", "Library order") }, { "name", T("srv_by_name", "Name") }, { "enabled", T("sort_enabled", "Enabled first") }, { "status", T("sort_problems", "Problems first") },
    { "size", T("sort_size", "Size") }, { "changed", T("sort_changed", "Last changed") }, { "installed", T("sort_installed", "Date installed") } }

--- Whether the list isn't all of it in library order (a search, a tab, a filter or a sort): drags need it to be.
function PZMMMods:filtering()
    local q = self.search and self.search:getText() or ""
    return q ~= "" or self.tab ~= "All" or self.favOnly or self.filterHidden or self.filterKind ~= nil or self.langOnly == true or self.filterTag ~= nil or (self.sortBy or "order") ~= "order"
end

--- The list's toolbar, as the app's: Group by category (PZMM's categories), Filters, Sort and New section after
--- the tabs; Clear filters, Check setup, Undo and Redo at the right.
function PZMMMods:drawTools(x, y, mx, my)
    self.toolBoxes = {}
    local function add(id, icon, label, tipText, on, off, primary)
        local w, over
        if primary then
            -- the blood-red one (Install N missing): bold label, white
            w = width(label) + 47
            over = mx >= x and mx < x + w and my >= y and my < y + BAR
            round(self, x, y, w, BAR, 6, over and C.accentHover or C.accent)
            outline(self, x, y, w, BAR, 6, C.accent)
            if icon then self:drawTextureScaled(icon, x + 13, y + math.floor((BAR - 14) / 2), 14, 14, 1, 1, 1, 1) end
            bold(self, label, x + 33, y + (BAR - H_SMALL) / 2, C.onAccent)
        else
            w, over = toolButton(self, icon, label, x, y, mx, my, on, off)
        end
        if over then tip(self, tipText) end
        self.toolBoxes[#self.toolBoxes + 1] = { id, x, y, w, BAR, off }
        return w
    end
    local sortName = SORTS[1][2]
    for _, o in ipairs(SORTS) do if o[1] == (self.sortBy or "order") then sortName = o[2] end end
    local left = {}
    left[#left + 1] = { "group", TB.layers, T("tool_group", "Group by category"), T("tool_group_tip_title", "Group by category") .. "\nOne section per category: Frameworks & libraries first, Translations last. Asks first if you have sections of your own." }
    local nf = (self.favOnly and 1 or 0) + (self.filterHidden and 1 or 0) + (self.filterKind and 1 or 0) + (self.langOnly and 1 or 0) + (self.filterTag and 1 or 0)
    left[#left + 1] = { "filters", TB.filter, nf > 0 and ("Filters (" .. nf .. ")") or "Filters", "Filters\nFavourites, hidden mods, kinds and tags. Search finds authors too.", nf > 0 }
    left[#left + 1] = { "sort", TB.sort, sortName, "Sort the list\nWithin each section. Drag mods in library order." }
    left[#left + 1] = { "section", TB.plus, T("tool_section", "New section"), T("tool_section_tip_title", "New section") .. "\nAt the end of the list, or under the picked mod's section." }
    left[#left + 1] = { "presets", TB.layers, T("tool_presets", "Presets"), T("tool_presets_tip_title", "Presets") .. "\nThe game's mod presets: load one, save what's on now, or add one a friend sent (copied text)." }
    -- several picked: what to do with them, instead
    local np = 0
    for _ in pairs(self.picked) do np = np + 1 end
    if np >= 2 then
        bold(self, np .. " selected", x, y + (BAR - H_SMALL) / 2, C.text)
        x = x + width(np .. " selected") + 12
        left = {
            { "bulkOn", TB.check, "Enable", "Enable\nTurn the picked mods on (and what they need)." },
            { "bulkOff", TB.x, "Disable", "Disable\nTurn just the picked mods off." },
            { "bulkMove", TB.layers, "Move to", "Move to\nA section, the top or the bottom of the list.", false, self:filtering() },
            { "bulkHide", TB.filter, self.filterHidden and "Show again" or "Hide", "Hide\nOut of the list (Filters > Hidden shows them). Mod Manager and PZMM hide them too." },
            { "bulkClear", TB.x, "Clear", "Clear\nUnpick them." },
        }
    end
    for _, b in ipairs(left) do
        x = x + add(b[1], b[2], b[3], b[4], b[5], b[6]) + 8
        if b[1] == "filters" and np < 2 then
            -- each active filter as a chip: click to clear it
            local active = {}
            if self.favOnly then active[#active + 1] = { "unfav", T("home_favourites", "Favourites") } end
            if self.filterHidden then active[#active + 1] = { "unhidden", T("filter_hidden", "Hidden") } end
            if self.filterKind then active[#active + 1] = { "uncat", self.filterKind } end
            if self.langOnly then active[#active + 1] = { "unlang", T("filter_lang", "In my language") } end
            if self.filterTag then active[#active + 1] = { "untag", self.filterTag } end
            local ch = BAR - 2
            local cy = y + 1
            for _, a in ipairs(active) do
                local cw = width(a[2]) + 10 + 6 + 12 + 8
                round(self, x, cy, cw, ch, 6, C.accent, 0.32)
                outline(self, x, cy, cw, ch, 6, C.accent, 0.7)
                text(self, a[2], x + 10, cy + (ch - H_SMALL) / 2, C.onSelected)
                if TB.x then self:drawTextureScaled(TB.x, x + cw - 20, cy + math.floor((ch - 12) / 2), 12, 12, 1, 1, 1, 1) end
                if mx >= x and mx < x + cw and my >= cy and my < cy + ch then tip(self, "Clear filter\n" .. a[2]) end
                self.toolBoxes[#self.toolBoxes + 1] = { a[1], x, cy, cw, ch }
                x = x + cw + 4
            end
            if #active > 0 then x = x + 4 end
        end
    end
    if np >= 2 then
        local label = T("tool_leave_favs", "Leave favourites")
        local lw = BOX + 8 + width(label) + 8
        checkbox(self, x, y + (BAR - BOX) / 2, self.leaveFavs)
        text(self, label, x + BOX + 8, y + (BAR - H_SMALL) / 2, C.text)
        if mx >= x and mx < x + lw and my >= y and my < y + BAR then tip(self, label .. "\nEnable and Disable skip your favourites.") end
        self.toolBoxes[#self.toolBoxes + 1] = { "leaveFavs", x, y, lw, BAR }
        x = x + lw + 8
    end
    local right = {}
    local upd = self:waitingUpdates()
    if self.app and #upd > 0 then
        right[#right + 1] = { "updateAll", TB.download, "Update all (" .. #upd .. ")", "Update all\nOpens PZMM, which updates the " .. #upd .. " Workshop items waiting for an update once you say yes.", false, false, true }
    end
    -- required mods that aren't installed: PZMM can install the ones Steam told us about
    local ids = self:missingItems()
    if self.app and #ids > 0 then
        right[#right + 1] = { "installMissing", TB.download, "Install missing mods (" .. #ids .. ")", "Install missing\nOpens PZMM, which installs the Workshop items your enabled mods need once you say yes.", false, false, true }
    end
    if self:filtering() then right[#right + 1] = { "clear", TB.x, T("tool_clear", "Clear filters"), T("tool_clear_tip_title", "Clear filters") .. "\nClear the search and filter, and sort by library order, to reorder." } end
    right[#right + 1] = { "check", TB.pulse, T("tool_check", "Check setup"), T("tool_check_tip_title", "Check setup") .. "\nChecks your enabled mods: what can't load, what's missing or off, what clashes, the load order and Steam's downloads. Each problem comes with its fix.", self.checkOpen }
    right[#right + 1] = { "undo", TB.undo, nil, "Undo\nUndo the last list change.", false, #self.undos == 0 }
    right[#right + 1] = { "redo", TB.redo, nil, "Redo", false, #self.redos == 0 }
    local _, _, lw = self:listBox()
    local rx = PAD + lw
    local lim = x
    for i = #right, 1, -1 do
        local b = right[i]
        rx = rx - (b[3] and width(b[3]) + 46 or BAR)
        if rx < lim then break end
        x = rx
        add(b[1], b[2], b[3], b[4], b[5], b[6], b[7])
        rx = rx - 2
    end
end

--- The Workshop items that enabled mods need and you aren't subscribed to (Steam's answer), once a second.
function PZMMMods:missingItems()
    local t = getTimestampMs()
    if self.missingAt and t - self.missingAt < 1000 then return self.missingIds or {} end
    self.missingAt = t
    local seen, ids = {}, {}
    if self.steam then
        for _, d in pairs(self.model.mods) do
            if d.isActive and #missingReqs(d) > 0 then
                local st = self:steamOf(d)
                for _, item in ipairs(st and st.missing or {}) do
                    if not seen[item] then
                        seen[item] = true
                        ids[#ids + 1] = item
                    end
                end
            end
        end
    end
    self.missingIds = ids
    return ids
end

--- The Workshop items Steam says have an update waiting, once a second.
function PZMMMods:waitingUpdates()
    local t = getTimestampMs()
    if self.updAt and t - self.updAt < 1000 then return self.updIds or {} end
    self.updAt = t
    local seen, ids = {}, {}
    for _, d in pairs(self.model.mods) do
        local st = self:steamOf(d)
        if st and st.state == "NeedsUpdate" then
            local ws = wsOf(d)
            if ws and not seen[ws] then
                seen[ws] = true
                ids[#ids + 1] = ws
            end
        end
    end
    self.updIds = ids
    return ids
end

function PZMMMods:onTool(id, mx, my)
    if id == "leaveFavs" then
        self.leaveFavs = not self.leaveFavs
    elseif id == "updateAll" then
        local ids = self:waitingUpdates()
        if #ids > 0 then self:request("install\t" .. table.concat(ids, ","), #ids .. " updates asked for.") end
    elseif id == "installMissing" then
        local ids = self:missingItems()
        if #ids > 0 then self:request("install\t" .. table.concat(ids, ","), #ids .. " items asked for.") end
    elseif id == "unfav" then
        self.favOnly, self.scroll = false, 0
        self:updateView()
    elseif id == "unhidden" then
        self.filterHidden, self.scroll = false, 0
        self:updateView()
    elseif id == "unlang" then
        self.langOnly, self.scroll = false, 0
        self:updateView()
    elseif id == "uncat" then
        self.filterKind, self.scroll = nil, 0
        self:updateView()
    elseif id == "untag" then
        self.filterTag, self.scroll = nil, 0
        self:updateView()
    elseif id == "group" then
        self:groupByCategory()
    elseif id == "filters" then
        local nh = 0
        for _ in pairs(self.hidden or {}) do nh = nh + 1 end
        local function redo() self.scroll = 0; self:updateView() end
        local items = {
            { T("filter_show", "Show"), heading = true },
            { T("home_favourites", "Favourites"), function() self.favOnly = not self.favOnly; redo() end, function() return self.favOnly == true end, keep = true },
            { T("filter_hidden", "Hidden") .. (nh > 0 and (" (" .. nh .. ")") or ""), function() self.filterHidden = not self.filterHidden; redo() end, function() return self.filterHidden == true end, keep = true },
            { T("filter_lang", "In my language"), function()
                self.langOnly = not self.langOnly
                self.langDone = false
                redo()
            end, function() return self.langOnly == true end, keep = true },
            { T("filter_kind", "Kind"), heading = true },
            { T("filter_any_kind", "Any kind"), function() self.filterKind = nil; redo() end, function() return self.filterKind == nil end, keep = true },
        }
        for _, c in ipairs(K.kindNames()) do
            items[#items + 1] = { c, function() self.filterKind = self.filterKind ~= c and c or nil; redo() end, function() return self.filterKind == c end, keep = true }
        end
        -- the tags PZMM has on mods
        local seen, tagList = {}, {}
        for _, list in pairs(self.tags or {}) do
            for _, t in ipairs(list) do
                if not seen[t:lower()] then
                    seen[t:lower()] = true
                    tagList[#tagList + 1] = t
                end
            end
        end
        table.sort(tagList, function(a, b) return a:lower() < b:lower() end)
        if #tagList > 0 then
            items[#items + 1] = { T("filter_tag", "Tag"), heading = true }
            items[#items + 1] = { T("filter_any_tag", "Any tag"), function() self.filterTag = nil; redo() end, function() return self.filterTag == nil end, keep = true }
            for _, t in ipairs(tagList) do
                local low = t:lower()
                items[#items + 1] = { t, function() self.filterTag = self.filterTag ~= low and low or nil; redo() end, function() return self.filterTag == low end, keep = true }
            end
        end
        self:menu(items, mx, my)
    elseif id == "sort" then
        local items = {}
        for _, o in ipairs(SORTS) do
            if o[1] ~= "installed" or self.hasInstalled then
                items[#items + 1] = { o[2], function() self.sortBy = o[1]; self:updateView() end, (self.sortBy or "order") == o[1] }
            end
        end
        self:menu(items, mx, my)
    elseif id == "section" then
        self:newSection()
    elseif id == "presets" then
        self:presetMenu(mx, my)
    elseif id == "bulkOn" or id == "bulkOff" then
        local snap = self:snapshot()
        local on = id == "bulkOn"
        local ons, offs, kept = {}, {}, 0
        for mid in pairs(self.picked) do
            local d = self.model.mods[mid]
            if d and d.isActive ~= on then
                if self.leaveFavs and d.favorite then
                    kept = kept + 1
                else
                    local l = on and ons or offs
                    l[#l + 1] = d
                end
            end
        end
        local skipped = self:setMany(ons, offs)
        self:rememberIfChanged(snap)
        if skipped > 0 then self:say(self:skipNote(skipped)) end
        if kept > 0 then self:say(kept .. (kept == 1 and " favourite" or " favourites") .. " left as " .. (kept == 1 and "it was." or "they were.")) end
    elseif id == "bulkMove" then
        local items = { { "Top of the list", function() self:moveMany(PZMMMods.MOVE_TOP) end } }
        for _, g in ipairs(self.lib and self.lib.sections or {}) do items[#items + 1] = { "Under " .. g, function() self:moveMany(g) end } end
        items[#items + 1] = { "Bottom of the list", function() self:moveMany(PZMMMods.MOVE_BOTTOM) end }
        self:menu(items, mx, my)
    elseif id == "bulkHide" then
        local hide = not self.filterHidden
        for mid in pairs(self.picked) do self.hidden[mid] = hide or nil end
        writeHidden(self.hidden)
        self.picked = {}
        self:updateView()
        self:say(hide and "Hidden: Filters > Hidden shows them." or "Shown again.")
    elseif id == "bulkClear" then
        self.picked = {}
    elseif id == "clear" then
        self.search:setText("")
        self.langOnly = false
        self.filterTag = nil
        self.tab, self.favOnly, self.filterHidden, self.filterKind, self.sortBy, self.scroll = "All", false, false, nil, "order", 0
        self:updateView()
    elseif id == "check" then
        self.checkOpen = not self.checkOpen
        self.findings, self.pfAt = nil, nil
    elseif id == "undo" or id == "redo" then
        self:undoRedo(id == "undo")
    end
end

-- the app's categories, in its section order (core pz/category.rs); categories.txt, which PZMM writes from
-- Workshop tags, says which each mod is in, and the rest are worked out here
K.CATEGORIES = { "Frameworks & libraries", "Gameplay", "Quality of life", "Interface", "Characters & traits", "Items & crafting",
    "Weapons", "Clothing & armour", "Vehicles", "Visuals & audio", "Maps", "Translations", "Other" }

--- PZMM's category for each mod (Zomboid/Lua/pzmm/categories.txt), and its section order. nil without it.
local function readCategories()
    local ok, cats = pcall(function()
        local r = getFileReader("pzmm/categories.txt", false)
        if not r then return nil end
        local cats = { of = {} }
        local line = r:readLine()
        while line do
            local id, c = line:match("^([^\t]+)\t(.+)$")
            if id == "order" then
                cats.order = {}
                for n in c:gmatch("[^\t]+") do cats.order[#cats.order + 1] = n end
            elseif id then
                cats.of[id] = c
            end
            line = r:readLine()
        end
        r:close()
        return cats
    end)
    return ok and cats or nil
end

--- How many mods need each mod (frameworks are what several need).
function PZMMMods:countNeeded()
    self.neededAll = {}
    for _, d in pairs(self.model.mods) do
        for id, _ in pairs(d.requireMods or {}) do self.neededAll[id] = (self.neededAll[id] or 0) + 1 end
    end
end

--- A mod PZMM hasn't categorised (no app, or new since): its mod.info category, else what its name says,
--- as PZMM tells a mod's kind without Workshop tags.
function PZMMMods:guessCategory(d)
    local cat = d.category or ""
    if cat == "map" then return "Maps" end
    if cat == "vehicle" then return "Vehicles" end
    local text = " " .. d.modId:lower() .. " " .. (d.name or ""):lower() .. " "
    local function says(words)
        for _, w in ipairs(words) do if text:find(w, 1, true) then return true end end
        return false
    end
    if says({ "translat", "tradu", "localiz", "locali" }) then return "Translations" end
    if (self.neededAll[d.modId] or 0) >= 2 or says({ "framework", " lib ", "library", " api ", "_api ", " core " }) then return "Frameworks & libraries" end
    if says({ " ui ", " ui)", "(ui", " hud", "interface", "menu" }) then return "Interface" end
    if says({ " map", "map " }) then return "Maps" end
    return "Other"
end

--- The app's categories in section order (for code above their local).
function PZMMMods:categoryList() return K.CATEGORIES end

--- A mod's category: PZMM's (categories.txt), else worked out here.
function PZMMMods:categoryOf(d)
    self.cats = self.cats or readCategories() or { of = {} }
    if not self.neededAll then self:countNeeded() end
    local c = self.cats.of[d.modId]
    return c or self:guessCategory(d)
end

--- Group by category, as the app does: one section per category in its order, each keeping the library's
--- order, replacing the sections there are (it asks first if any are your own).
function PZMMMods:groupByCategory()
    if self.app and not self.lib then
        self:say("Open PZMM once: it brings your profile's sections here to sort.")
        return
    end
    local cats = readCategories() or { of = {} }
    local order = cats.order or K.CATEGORIES
    local known = {}
    for _, n in ipairs(order) do known[n] = true end
    local own = {}
    for _, n in ipairs(self.lib and self.lib.sections or {}) do
        if not known[n] and n ~= "Other" then own[#own + 1] = n end
    end
    self:countNeeded()
    local function group()
        self:remember()
        local pos = self.lib and self.lib.pos or {}
        local by = {}
        for _, d in pairs(self.model.mods) do
            local c = cats.of[d.modId]
            if not (c and known[c]) then c = self:guessCategory(d) end
            if not known[c] then c = order[#order] end
            by[c] = by[c] or {}
            table.insert(by[c], d)
        end
        local lines, n = { "# PZMM profile: " .. ((self.lib and self.lib.profile) or "In game") }, 0
        for _, c in ipairs(order) do
            local mods = by[c]
            if mods then
                table.sort(mods, function(a, b)
                    local pa, pb = pos[a.modId] or 1e9, pos[b.modId] or 1e9
                    if pa ~= pb then return pa < pb end
                    return byName(a, b)
                end)
                lines[#lines + 1] = "# --- " .. c
                for _, d in ipairs(mods) do lines[#lines + 1] = (d.isActive and "+" or "-") .. d.modId end
                n = n + 1
            end
        end
        self.collapsed = {}
        if self:writeLibrary(lines) then self:toApp("library\t" .. libBody(lines), "Grouped into " .. n .. " sections. Undo puts yours back.") end
    end
    if #own == 0 then return group() end
    local shown = {}
    for i = 1, math.min(4, #own) do shown[i] = own[i] end
    if #own > 4 then shown[#shown + 1] = "..." end
    self:ask({ title = "Group by category replaces your own sections with category sections. Undo puts them back.", lines = shown, choices = {
        { "Group by category", group },
    } })
end

-- --- Check setup ---------------------------------------------------------------------------------------

--- PZMM's last Check setup (Zomboid/Lua/pzmm/findings.txt): at=<secs>, then level, mod, text and fix pairs.
local function readFindings()
    local ok, f = pcall(function()
        local r = getFileReader("pzmm/findings.txt", false)
        if not r then return nil end
        local f = { list = {} }
        local line = r:readLine()
        while line do
            local at = line:match("^at=(%d+)")
            if at then
                f.at = tonumber(at)
            else
                local p, start = {}, 1
                while true do
                    local i = line:find("\t", start, true)
                    if not i then p[#p + 1] = line:sub(start); break end
                    p[#p + 1] = line:sub(start, i - 1)
                    start = i + 1
                end
                if #p >= 3 then
                    local e = { level = p[1], mod = p[2] ~= "-" and p[2] or nil, text = p[3], fixes = {} }
                    for i = 4, #p - 1, 2 do e.fixes[#e.fixes + 1] = { id = p[i], label = p[i + 1] } end
                    f.list[#f.list + 1] = e
                end
            end
            line = r:readLine()
        end
        r:close()
        return f
    end)
    return ok and f or nil
end

--- A finding's "It's fine" key: its level, the mod it is about and its first words without the numbers (so a
--- count changing doesn't make it a new finding), with no tab or line break (it is kept one to a line).
local function stableKey(level, id, txt)
    local words = {}
    for w in tostring(txt or ""):gmatch("%S+") do
        w = w:gsub("%d+", "")
        if w ~= "" then words[#words + 1] = w end
        if #words >= 6 then break end
    end
    return ((level .. "|" .. tostring(id or "-") .. "|" .. table.concat(words, " ")):gsub("[\t\r\n]", " "))
end

--- What the game itself can check, as the app's Check setup: { level, text, fixes = { { label, fn } } },
--- errors first.
function PZMMMods:checkSetup()
    local mods, out = self.model.mods, {}
    -- grp = { tpl = "%1 can't load...", d = the mod }: the same finding on several mods becomes one line
    local function add(level, text, fixes, id, grp) out[#out + 1] = { level = level, text = text, fixes = fixes or {}, key = stableKey(level, id, text), grp = grp } end
    local function off(d) return { "Disable " .. d.name, function() self:toggle(d) end } end
    local pairsSeen = {}
    local names = {}
    for _, d in pairs(mods) do names[#names + 1] = d end
    table.sort(names, byName)
    for _, d in ipairs(names) do
        if d.isActive then
            if not d.isAvailable then
                local range = ""
                pcall(function()
                    local lo, hi = d.modInfo:getVersionMin(), d.modInfo:getVersionMax()
                    if lo or hi then range = " It needs game version " .. (lo and lo:toString() or "any") .. (hi and (" to " .. hi:toString()) or " or later") .. "." end
                end)
                add("error", d.name .. " can't load on this build of the game." .. range, { off(d) }, d.modId, { tpl = "%1 can't load on this build of the game." .. range, d = d })
            end
            -- Steam answers a Workshop item that's gone, banned or hidden with an empty title
            local st = self:steamOf(d)
            if st and (st.title or "") == "" then
                local why = " its Workshop item is gone, banned or hidden: it gets no updates, and servers can't download it."
                add("warn", d.name .. ":" .. why, { off(d) }, d.modId .. "~gone", { tpl = "%1:" .. why, d = d })
            end
            for _, q in ipairs(self:rulesOf(d).requires) do
                local dep = mods[q]
                if not dep then
                    local fixes = {}
                    local st = self:steamOf(d)
                    for _, item in ipairs(st and st.missing or {}) do
                        local title = (self.steam[item] and self.steam[item].title) or ("Workshop item " .. item)
                        fixes[#fixes + 1] = self.app and { "Install " .. title, function() self:request("install\t" .. item, title .. " asked for.") end }
                            or { "Subscribe to " .. title, function() self:openWorkshop(item) end }
                    end
                    fixes[#fixes + 1] = off(d)
                    add("error", d.name .. " needs " .. q .. ", which isn't installed.", fixes, d.modId .. ">" .. q)
                elseif not dep.isActive then
                    add(dep.isAvailable and "warn" or "error", d.name .. " needs " .. dep.name .. ", which is off.",
                        dep.isAvailable and { { "Enable " .. dep.name, function() self:toggle(dep) end, safe = "on", mod = dep } } or { off(d) }, d.modId .. ">" .. q)
                end
            end
            for id, _ in pairs(d.incompatibleWith or {}) do
                local o = mods[id]
                local key = d.modId < id and (d.modId .. "|" .. id) or (id .. "|" .. d.modId)
                if o and o.isActive and not pairsSeen[key] then
                    pairsSeen[key] = true
                    add("error", d.name .. " and " .. o.name .. " can't both be on.", { off(d), off(o) }, key)
                end
            end
            local st = self:steamOf(d)
            local look = K.look(st)
            if look then
                local why = look[3]:gsub("^[^\n]*\n", "")
                local fixes
                if st.pzmm then
                    local ws = wsOf(d)
                    fixes = { { T("feed_update", "Update"), function() if self.app then self:request("install\t" .. ws, d.name .. " update asked for.") else self:openWorkshop(ws) end end } }
                end
                add("warn", d.name .. ": " .. why, fixes, d.modId, { tpl = "%1: " .. why, d = d })
            end
        end
    end
    local wrong = 0
    for _, p in pairs(self.oprob or {}) do if p[2] then wrong = wrong + 1 end end
    if wrong > 0 then
        add("warn", wrong .. (wrong == 1 and " mod loads" or " mods load") .. " in the wrong order for its requirements or load rules.", { { "Fix load order", function() self:fixOrder() end, safe = "order" } })
    end
    -- Java mods without ZombieBuddy running
    local zbOn = false
    if ZombieBuddy and ZombieBuddy.getVersion then pcall(function() zbOn = ZombieBuddy.getVersion() ~= nil end) end
    if not zbOn then
        local list, done = self:javaMods()
        out.scanning = not done
        local on = 0
        for _, j in ipairs(list) do if j.d.isActive then on = on + 1 end end
        if done and on > 0 then
            add("warn", on .. (on == 1 and " enabled mod ships" or " enabled mods ship") .. " Java, but ZombieBuddy isn't running: " .. (on == 1 and "it" or "they") .. " won't fully load.",
                { { "Java mods", function() self.page = "java" end } })
        end
    end
    -- identical findings (the same words about different mods) in one line, its fix turning them all off
    do
        local merged, by = {}, {}
        for _, f in ipairs(out) do
            local g = f.grp and by[f.level .. f.grp.tpl]
            if g then
                g.ds[#g.ds + 1] = f.grp.d
            else
                merged[#merged + 1] = f
                if f.grp then by[f.level .. f.grp.tpl] = { f = f, ds = { f.grp.d } } end
            end
        end
        for _, g in pairs(by) do
            if #g.ds > 1 then
                local names = {}
                for i, d in ipairs(g.ds) do names[i] = d.name end
                local list = #names == 2 and (names[1] .. " and " .. names[2]) or (table.concat(names, ", ", 1, #names - 1) .. " and " .. names[#names])
                g.f.text = g.f.grp.tpl:gsub("%%1", function() return list end)
                local ds = g.ds
                g.f.fixes = g.f.level == "error" and { { "Turn off " .. #ds .. " mods", function()
                    for _, d in ipairs(ds) do if d.isActive then self:toggle(d) end end
                end } } or {}
                g.f.key = stableKey(g.f.level, g.ds[1].modId .. "+", g.f.text)
            end
        end
        local scanning = out.scanning
        out = merged
        out.scanning = scanning
    end
    local rank = { error = 1, warn = 2, info = 3 }
    for i, f in ipairs(out) do f.n = i end
    table.sort(out, function(a, b)
        if rank[a.level] ~= rank[b.level] then return rank[a.level] < rank[b.level] end
        return a.n < b.n
    end)
    return out
end

K.LEVEL_COLOUR = { error = "danger", warn = "warn", info = "muted" }
-- the app's severities: what each group is called, and what it means
K.LEVEL_NAME = { error = T("check_broken", "Broken"), warn = T("check_check", "Check"), info = T("check_low", "Low") }
K.LEVEL_HINT = {
    error = T("check_broken_hint", "The game won't load these, or won't start"),
    warn = T("check_check_hint", "May misbehave in game"),
    info = T("check_low_hint", "Worth knowing; nothing to fix"),
}


--- The side panel's Check setup: what the game finds, each with its fixes, then PZMM's last check (its own
--- fixes run in PZMM). Returns the y under it.
local function pfKey(f) return "pzmm:" .. stableKey(f.level, f.mod, f.text) end
function PZMMMods:drawCheck(x, y, w, bottom)
    local mx, my = self:getMouseX(), self:getMouseY()
    -- Close, with a quiet Check again beside it
    local cxw = width("Close") + 28
    local _, _, cover = flatButton(self, "Close", x + w - cxw, y, mx, my, "quiet")
    if cover then tip(self, "Close\nBack to the load order.") end
    self.hits[#self.hits + 1] = { kind = "fn", fn = function() self.checkOpen = false end, x = x + w - cxw, y = y, w = cxw, h = BAR }
    local again = T("check_again", "Check again")
    local aw = width(again) + 48
    local ax = x + w - cxw - 8 - aw
    local _, _, aover = flatButton(self, again, ax, y, mx, my, "quiet", TB.pulse)
    if aover then tip(self, "Check again\nLooks at your enabled mods once more.") end
    self.hits[#self.hits + 1] = { kind = "fn", fn = function() self.findings, self.pfAt = nil, nil end, x = ax, y = y, w = aw, h = BAR }
    bold(self, "Check setup", x, y + (BAR - H_MEDIUM) / 2, C.text, MEDIUM)
    y = y + BAR + 10
    y = self:drawBisect(x, y, w, mx, my)
    -- not kept while the Java scan is still running: it would miss what the scan finds
    if self.findings and self.findings.scanning then
        local _, done = self:javaMods()
        if done then self.findings = nil end
    end
    self.findings = self.findings or self:checkSetup()
    local list = self.findings
    self.ignored = self.ignored or self:loadIgnored()
    -- a finding marked "It's fine" is hidden (kept in Zomboid/Lua/pzmm/ignored.txt) unless Show hidden is on.
    -- Its sentence at the left, its fixes in a column at the right, a line under it.
    local function finding(level, txt, fixes, key)
        local ign = self.ignored[key]
        local c = ign and C.muted or C[K.LEVEL_COLOUR[level] or "muted"]
        local ind = 12
        local all = {}
        for _, f in ipairs(fixes) do all[#all + 1] = f end
        -- "It's fine" only for what isn't an error
        if level ~= "error" then
            all[#all + 1] = { ign and T("check_unfine", "Show again") or T("check_fine", "It's fine"), function()
                self.ignored[key] = (not ign) or nil
                self:saveIgnored()
            end, quiet = true }
        end
        local colW = 0
        for _, f in ipairs(all) do colW = math.max(colW, width(f[1]) + 28) end
        colW = math.min(colW, math.floor(w * 0.5))
        local lines = wrap(txt, w - ind - colW - 12)
        local th = #lines * (H_SMALL + 2)
        local bh = #all * (BAR + 6) - 6
        local rh = math.max(th, bh)
        local y0 = y
        for i, l in ipairs(lines) do text(self, l, x + ind, y + (i - 1) * (H_SMALL + 2), ign and C.muted or C.text) end
        local by = y
        for i, f in ipairs(all) do
            local label = fit(f[1], colW - 28)
            local bw = width(label) + 28
            local bx = x + w - bw
            local _, _, over = flatButton(self, label, bx, by, mx, my, f.quiet and "quiet" or (i == 1) and "primary" or nil)
            if over and f.quiet then tip(self, ign and "Show again\nPuts this finding back in the list." or "It's fine\nHides this finding. Show hidden brings it back.") end
            self.hits[#self.hits + 1] = { kind = "fn", fn = function() f[2](); self.findings = nil end, x = bx, y = by, w = bw, h = BAR }
            by = by + BAR + 6
        end
        -- a 2 px bar down its left in the level's colour (none for info)
        if level == "error" or level == "warn" then rect(self, x, y0, 2, rh, c, 1) end
        y = y + rh + 8
        rect(self, x, y, w, 1, C.line, 1)
        y = y + 9
    end
    local visible, hidden = {}, 0
    for _, f in ipairs(list) do
        if self.ignored[f.key] then
            hidden = hidden + 1
            if self.showIgnored then visible[#visible + 1] = f end
        else
            visible[#visible + 1] = f
        end
    end
    for _, f in ipairs(self.pf and self.pf.list or {}) do
        if self.ignored[pfKey(f)] then hidden = hidden + 1 end
    end
    if #visible == 0 then
        text(self, T("check_none", "No problems found."), x, y, C.muted)
        y = y + H_SMALL + 16
    else
        local e, wn = 0, 0
        for _, f in ipairs(visible) do
            if not self.ignored[f.key] then
                if f.level == "error" then e = e + 1 elseif f.level == "warn" then wn = wn + 1 end
            end
        end
        local nlive = 0
        for _, f in ipairs(visible) do if not self.ignored[f.key] then nlive = nlive + 1 end end
        text(self, nlive == 1 and T("check_summary_one", "1 thing to look at") or T("check_summary", "%1 things to look at", nlive), x, y, C.muted)
        y = y + H_SMALL + 12
        local live = {}
        for _, f in ipairs(visible) do if not self.ignored[f.key] then live[#live + 1] = f end end
        local ons, order, n = self:safeFixes(live)
        if n > 0 then
            local label = T("check_fix_safe", "Fix %1 safely", n)
            local bw = width(label) + 28
            local _, _, over = flatButton(self, label, x, y, mx, my, "primary")
            if over then tip(self, label .. "\nTurns on what your enabled mods need and fixes the load order. Nothing is turned off or removed; Undo takes it back.") end
            self.hits[#self.hits + 1] = { kind = "fn", fn = function() self:fixSafely(ons, order) end, x = x, y = y, w = bw, h = BAR }
            y = y + BAR + 12
        end
        -- grouped by how bad: each group folds (the low ones start folded), with a tone pill, how many and what it means
        self.checkFold = self.checkFold or { info = true }
        for _, lv in ipairs({ "error", "warn", "info" }) do
            local group = {}
            for _, f in ipairs(visible) do if f.level == lv then group[#group + 1] = f end end
            if #group > 0 then
                local closed = self.checkFold[lv]
                local tone = C[K.LEVEL_COLOUR[lv] or "muted"]
                local hh = math.max(22, H_SMALL + 6)
                local chev = closed and K.CHEV_RIGHT or K.CHEV_DOWN
                if chev then self:drawTextureScaled(chev, x, y + math.floor((hh - 14) / 2), 14, 14, 1, C.muted.r, C.muted.g, C.muted.b) end
                local name = K.LEVEL_NAME[lv]
                local pw = width(name) + 20
                round(self, x + 20, y + math.floor((hh - 20) / 2), pw, 20, 10, tone, 0.22)
                bold(self, name, x + 30, y + (hh - H_SMALL) / 2, tone)
                local count = tostring(#group)
                local cx2 = x + 20 + pw + 8
                text(self, count, cx2, y + (hh - H_SMALL) / 2, C.muted)
                local hx2 = cx2 + width(count) + 8
                text(self, fit(K.LEVEL_HINT[lv], x + w - hx2), hx2, y + (hh - H_SMALL) / 2, C.muted)
                local lvx = lv
                self.hits[#self.hits + 1] = { kind = "fn", fn = function() self.checkFold[lvx] = not self.checkFold[lvx] or nil end, x = x, y = y, w = w, h = hh }
                y = y + hh + 8
                if not closed then
                    for _, f in ipairs(group) do finding(f.level, f.text, f.fixes, f.key) end
                end
            end
        end
    end
    if hidden > 0 then
        local label = T("check_hidden", "%1 hidden", hidden)
        local bw = width(label) + 28
        flatButton(self, label, x, y, mx, my, "quiet")
        self.hits[#self.hits + 1] = { kind = "fn", fn = function() self.showIgnored = not self.showIgnored end, x = x, y = y, w = bw, h = BAR }
        y = y + BAR + 12
    end
    -- PZMM's own check: its last findings (fixes run in PZMM), and a fresh run
    if self.app then
        rect(self, x, y, w, 1, C.line, 1)
        y = y + 12
        bold(self, "From PZMM", x, y, C.text)
        y = y + H_SMALL + 4
        -- drawn every frame: PZMM's findings.txt is read at most every 2 s
        local t = getTimestampMs()
        if not self.pfAt or t - self.pfAt > 2000 then self.pfAt, self.pf = t, readFindings() end
        local pf = self.pf
        if not (pf and pf.at and pf.at > 0) then
            text(self, "No check yet.", x, y, C.muted)
            y = y + H_SMALL + 10
        else
            local mins = math.floor((now() - pf.at) / 60)
            text(self, "Its last check, " .. (mins < 1 and "just now" or mins < 60 and (mins .. " min ago") or mins < 1440 and (math.floor(mins / 60) .. " h ago") or (math.floor(mins / 1440) .. " days ago")) .. ": server configs, files and more.", x, y, C.muted)
            y = y + H_SMALL + 10
            local shown = 0
            for _, f in ipairs(pf.list) do
                local key = pfKey(f)
                if not self.ignored[key] or self.showIgnored then
                    shown = shown + 1
                    local fixes = {}
                    for _, fx in ipairs(f.fixes) do
                        fixes[#fixes + 1] = { fx.label, function() self:request("fix\t" .. fx.id, fx.label .. " asked for.") end }
                    end
                    finding(f.level, f.text, fixes, key)
                end
            end
            if shown == 0 then
                text(self, "PZMM found nothing wrong.", x, y, C.muted)
                y = y + H_SMALL + 12
            end
        end
        local label = T("check_full", "Run PZMM's full check")
        local bw = width(label) + 28
        local _, _, over = flatButton(self, label, x, y, mx, my)
        if over then tip(self, label .. "\nOpens PZMM on Check setup: it also checks server configs, overlapping files and game files.") end
        self.hits[#self.hits + 1] = { kind = "fn", fn = function() self:request("check", "Check setup asked for.") end, x = x, y = y, w = bw, h = BAR }
        y = y + BAR
    end
    return y
end

--- Findings marked "It's fine": Zomboid/Lua/pzmm/ignored.txt, one key per line. Returns the set.
function PZMMMods:loadIgnored()
    local set = {}
    pcall(function()
        local r = getFileReader("pzmm/ignored.txt", false)
        if not r then return end
        local line = r:readLine()
        while line do
            if line ~= "" then set[line] = true end
            line = r:readLine()
        end
        r:close()
    end)
    return set
end
function PZMMMods:saveIgnored()
    local keys = {}
    for k in pairs(self.ignored or {}) do keys[#keys + 1] = (k:gsub("[\t\r\n]", " ")) end
    table.sort(keys)
    pcall(function()
        local w = getFileWriter("pzmm/ignored.txt", true, false)
        w:write(table.concat(keys, "\n") .. (#keys > 0 and "\n" or ""))
        w:close()
    end)
end

--- The fixes in `list` that only turn mods on or reorder: the mods to turn on, whether to fix the load order,
--- and how many fixes that is.
function PZMMMods:safeFixes(list)
    local ons, seen, order, n = {}, {}, false, 0
    for _, f in ipairs(list) do
        for _, fx in ipairs(f.fixes) do
            if fx.safe == "on" and fx.mod and not seen[fx.mod.modId] then
                seen[fx.mod.modId] = true
                ons[#ons + 1] = fx.mod
                n = n + 1
            elseif fx.safe == "order" and not order then
                order = true
                n = n + 1
            end
        end
    end
    return ons, order, n
end

--- The game's presets (pz_modlist_settings.cfg, through its model): load, save, add from the clipboard, delete.
function PZMMMods:presetMenu(mx, my)
    local names = {}
    for name, _ in pairs(self.model.presets or {}) do names[#names + 1] = name end
    table.sort(names, function(a, b) return a:lower() < b:lower() end)
    local items = {}
    for _, name in ipairs(names) do items[#items + 1] = { "Load " .. name, function() self:loadPreset(name) end } end
    if #items > 0 then items[#items + 1] = false end
    items[#items + 1] = { "Save what's on as a preset", function()
        self:prompt("Save as preset", (self.lib and self.lib.profile) or "My mods", "Save", function(name)
            name = (name or ""):gsub("^%s+", ""):gsub("%s+$", ""):gsub("[:\r\n]", " ")
            if name == "" then return end
            local data = {}
            for id, d in pairs(self.model.mods) do if d.isActive then data[id] = id end end
            self.model.presets[name] = data
            self:savePresets()
            self:say(name .. " saved as a preset.")
        end)
    end }
    items[#items + 1] = { "Add a preset from copied text", function()
        local line = ""
        pcall(function() line = tostring(Clipboard.getClipboard() or "") end)
        line = line:gsub("[\r\n].*$", "")
        local name, ids = line:match("^([^:]+):(.*)$")
        if not name or name:gsub("%s", "") == "" then
            self:say("Copy a preset first: a name, a colon, then mod IDs with ; after each.")
            return
        end
        local data, n = {}, 0
        for id in ids:gmatch("[^;]+") do data[id], n = id, n + 1 end
        self.model.presets[name] = data
        self:savePresets()
        self:say(name .. " added: " .. n .. " mods. Load it from Presets.")
    end }
    if #names > 0 then
        items[#items + 1] = { "Copy a preset...", function()
            local sub = {}
            for _, name in ipairs(names) do
                sub[#sub + 1] = { name, function()
                    pcall(function() Clipboard.setClipboard(self.model:getPresetShareText(name)) end)
                    self:say(name .. " copied: paste it to a friend, or into Presets > Add.")
                end }
            end
            self:menu(sub, mx, my)
        end }
        items[#items + 1] = { "Delete a preset...", function()
            local sub = {}
            for _, name in ipairs(names) do
                sub[#sub + 1] = { name, function()
                    self:ask({ title = "Delete the preset " .. name .. "?", choices = { { "Delete", function()
                        self.model.presets[name] = nil
                        self:savePresets()
                        self:say(name .. " deleted.")
                    end } } })
                end, danger = true }
            end
            self:menu(sub, mx, my)
        end, danger = true }
    end
    self:menu(items, mx, my)
end

--- Load a preset: its installed mods on and the rest off, as the game's own Presets do. Undo takes it back.
function PZMMMods:loadPreset(name)
    local data = self.model.presets[name]
    if not data then return end
    local snap = self:snapshot()
    local missing = 0
    for id, _ in pairs(data) do if not self.model.mods[id] then missing = missing + 1 end end
    local ons, offs = {}, {}
    for id, d in pairs(self.model.mods) do
        if data[id] and not d.isActive and d.isAvailable then
            ons[#ons + 1] = d
        elseif not data[id] and d.isActive then
            offs[#offs + 1] = d
        end
    end
    local skipped = self:setMany(ons, offs)
    self:rememberIfChanged(snap)
    self:say(name .. " loaded" .. (missing > 0 and (": " .. missing .. " of its mods aren't installed.") or ".") .. (skipped > 0 and (" " .. self:skipNote(skipped)) or ""))
end

-- sentinels, so a section really called "top" or "bottom" isn't taken for them
PZMMMods.MOVE_TOP, PZMMMods.MOVE_BOTTOM = {}, {}

--- Move the picked mods, in their list order, to the end of section `g` (or the top or bottom of the list).
function PZMMMods:moveMany(g)
    if not self:ensureLibrary() then return end
    local all, lines, moving = self:libLines(), {}, {}
    for _, l in ipairs(all) do
        local id = l:match("^[+-](.+)$")
        if id and self.picked[id] then moving[#moving + 1] = l else lines[#lines + 1] = l end
    end
    -- a picked mod the library doesn't list yet (installed since) joins it, as a drop does
    local have, extra = {}, {}
    for _, l in ipairs(moving) do have[l:match("^[+-](.+)$")] = true end
    for id in pairs(self.picked) do
        local d = self.model.mods[id]
        if d and not have[id] then extra[#extra + 1] = d end
    end
    table.sort(extra, byName)
    for _, d in ipairs(extra) do moving[#moving + 1] = (d.isActive and "+" or "-") .. d.modId end
    local at
    if g == PZMMMods.MOVE_TOP then
        -- right under the first section's line: above it they'd fall into the implicit "Other", shown last
        at = 1
        while lines[at] and lines[at]:match("^# PZMM profile:") do at = at + 1 end
        for i, l in ipairs(lines) do
            if sectionName(l) then at = i + 1; break end
        end
    elseif g == PZMMMods.MOVE_BOTTOM then
        at = #lines + 1
    else
        at = sectionEnd(lines, g)
    end
    for k, l in ipairs(moving) do table.insert(lines, at + k - 1, l) end
    if table.concat(lines, "\n") == table.concat(all, "\n") then return end
    self:remember()
    self.picked = {}
    if self:writeLibrary(lines) then self:toApp("library\t" .. libBody(lines), #moving .. " mods moved.") end
end

--- One mod to the top or bottom of the list (the whole list, in library order).
function PZMMMods:moveOne(d, g)
    if self:filtering() then return self:say("Clear the search and filters, and sort by library order, to move a mod: it moves in the whole list.") end
    local was = self.picked
    self.picked = { [d.modId] = true }
    self:moveMany(g)
    self.picked = was
end

--- A new section at the end of the list, or under the picked mod's section, as the app's Add separator.
function PZMMMods:newSection()
    local under = self.selected and self.lib and self.lib.of[self.selected.modId]
    local taken, name = {}, "New section"
    for _, n in ipairs(self.lib and self.lib.sections or {}) do taken[n:lower()] = true end
    local n = 2
    while taken[name:lower()] do name = "New section " .. n; n = n + 1 end
    self:prompt("New section" .. (under and (", under " .. under) or ""), name, "Add", function(typed)
        if not self:ensureLibrary() then return end
        typed = self:sectionNameOk(typed)
        if not typed then return end
        self:remember()
        local lines = self:libLines()
        local g = under or self.lib.sections[#self.lib.sections]
        local at = g and sectionEnd(lines, g) or #lines + 1
        table.insert(lines, at, "# --- " .. typed)
        if self:writeLibrary(lines) then
            if g then self:toApp("section\tadd\t" .. typed .. "\t" .. g, typed .. " added.") else self:toApp("library\t" .. libBody(lines), typed .. " added.") end
        end
    end, nil, 60)
end

--- The list as it is now, for Undo: the library's lines and which mods are on.
function PZMMMods:snapshot()
    return { lines = self.lib and self:libLines() or nil, order = self:activeOrder(), profile = self.lib and self.lib.profile }
end

--- Before a list change that happens: Undo can go back to it (`snap`, taken before), and Redo starts over.
function PZMMMods:remember(snap)
    self.undos[#self.undos + 1] = snap or self:snapshot()
    if #self.undos > 50 then table.remove(self.undos, 1) end
    self.redos = {}
end

--- remember(snap) when the mods on or their order are not what snap had (a change that may do nothing).
function PZMMMods:rememberIfChanged(snap)
    if table.concat(snap.order, "\n") ~= table.concat(self:activeOrder(), "\n") then self:remember(snap) end
end

--- Undo (or Redo) the last list change: mods back on or off, and the sections as they were, in PZMM's profile too.
function PZMMMods:undoRedo(undo)
    local from, to = undo and self.undos or self.redos, undo and self.redos or self.undos
    local snap = table.remove(from)
    if not snap then return end
    -- taken in another PZMM profile: its sections aren't these
    if snap.lines and snap.profile ~= (self.lib and self.lib.profile) then return self:say("Can't undo that: PZMM's profile changed since.") end
    to[#to + 1] = self:snapshot()
    if table.concat(snap.order, "\n") ~= table.concat(self:activeOrder(), "\n") then self:setOrder(snap.order) end
    if snap.lines and self:writeLibrary(snap.lines) then
        self:toApp("library\t" .. libBody(snap.lines), undo and "Undone." or "Redone.")
    else
        self:updateView()
        self:say(undo and "Undone." or "Redone.")
    end
end

-- --- the load order, as the app's order.ts: require, loadModAfter, loadModBefore ----------------------------

local function javaList(l)
    local out = {}
    if l then for i = 0, l:size() - 1 do out[#out + 1] = l:get(i) end end
    return out
end

-- Mod Load Order Sorter's rules (Zomboid/Lua/sorting_rules.txt), by mod ID: { after, before, first, last }
-- (loaded by PZMMMods:loadRuleFile; first and last are "on", "category" or "")
local FILE_RULES = {}

--- A mod's load rules from its mod.info and the rules file, read once.
local function rules(d)
    if not d.rules then
        local r = { requires = {}, after = {}, before = {}, infoAfter = {}, infoBefore = {} }
        pcall(function()
            r.requires = javaList(d.modInfo:getRequire())
            r.after = javaList(d.modInfo:getLoadAfter())
            r.before = javaList(d.modInfo:getLoadBefore())
        end)
        for i, id in ipairs(r.after) do r.infoAfter[i] = id end
        for i, id in ipairs(r.before) do r.infoBefore[i] = id end
        local f = FILE_RULES[d.modId]
        if f then
            for _, id in ipairs(f.after) do
                local have = false
                for _, o in ipairs(r.after) do if o == id then have = true end end
                if not have then r.after[#r.after + 1] = id end
            end
            for _, id in ipairs(f.before) do
                local have = false
                for _, o in ipairs(r.before) do if o == id then have = true end end
                if not have then r.before[#r.before + 1] = id end
            end
        end
        d.rules = r
    end
    return d.rules
end

--- `order` with every requirement and loadModAfter / loadModBefore rule holding, moving as little as possible:
--- a stable topological sort (ties keep their order; a cycle keeps its order).
local function sortLoadOrder(order, mods)
    local index, before = {}, {}
    for i, id in ipairs(order) do index[id], before[i] = i, {} end
    for i, id in ipairs(order) do
        local d = mods[id]
        if d then
            local r = rules(d)
            for _, list in ipairs({ r.requires, r.after }) do
                for _, a in ipairs(list) do
                    local j = index[a]
                    if j and j ~= i then before[i][j] = true end
                end
            end
            for _, b in ipairs(r.before) do
                local j = index[b]
                if j and j ~= i then before[j][i] = true end
            end
        end
    end
    local placed, out = {}, {}
    while #out < #order do
        local pick
        for i = 1, #order do
            if not placed[i] then
                local ready = true
                for j in pairs(before[i]) do
                    if not placed[j] then ready = false; break end
                end
                if ready then pick = i; break end
            end
        end
        if not pick then
            for i = 1, #order do
                if not placed[i] then pick = i; break end
            end
        end
        placed[pick] = true
        out[#out + 1] = order[pick]
    end
    return out
end

--- What each mod needs (requirements, loadModAfter) moved from below it to just above it, and the mod above
--- anything it must load before.
local function pullUp(order, mods)
    local list = {}
    for i, id in ipairs(order) do list[i] = id end
    local at = {}
    local function reindex()
        at = {}
        for k, id in ipairs(list) do at[id] = k end
    end
    -- ponytail: a cycle (A needs B, B loads after A) would swap forever; 50 passes is plenty for real lists
    for _ = 1, 50 do
        local moved = false
        reindex()
        local i = 1
        while i <= #list do
            local d = mods[list[i]]
            if d then
                local r = rules(d)
                for _, deps in ipairs({ r.requires, r.after }) do
                    for _, dep in ipairs(deps) do
                        local j = at[dep]
                        if j and j > i then
                            table.insert(list, i, table.remove(list, j))
                            i = i + 1
                            moved = true
                            reindex()
                        end
                    end
                end
                local first
                for _, b in ipairs(r.before) do
                    local j = at[b]
                    if j and j < i and (not first or j < first) then first = j end
                end
                if first then
                    table.insert(list, first, table.remove(list, i))
                    moved = true
                    reindex()
                end
            end
            i = i + 1
        end
        if not moved then break end
    end
    return list
end

-- the folders whose files the game lets a mod list (not their subfolders): where mods most often replace
-- each other's Lua and scripts
K.OVERLAP_DIRS = { "media/lua/client", "media/lua/server", "media/lua/shared", "media/scripts" }

--- PZMM's overlaps (overlaps.txt: "<id> TAB <loses to> TAB <wins over>", every media file compared), the enabled
--- mods only (it can lag a toggle); nil without the file. Read again every 2 seconds.
function PZMMMods:orderKey()
    if self.orderKeyOf ~= self.order then
        self.orderKeyOf, self.orderKeyText = self.order, table.concat(self.order or {}, ";")
    end
    return self.orderKeyText
end

function PZMMMods:appOverlaps()
    local t = getTimestampMs()
    local c = self.ovApp
    if c and t - c.at < 2000 and c.key == self:orderKey() then return c.of end
    local on, of, found = {}, {}, false
    for _, id in ipairs(self.order or {}) do on[id] = true end
    pcall(function()
        local r = getFileReader("pzmm/overlaps.txt", false)
        if not r then return end
        found = true
        local line = r:readLine()
        while line do
            local id, loses, wins = line:match("^([^\t]+)\t([^\t]*)\t?(.*)$")
            if id and on[id] then
                local o = { wins = {}, loses = {} }
                for x in loses:gmatch("[^,]+") do if on[x] then o.loses[x] = 1 end end
                for x in wins:gmatch("[^,]+") do if on[x] then o.wins[x] = 1 end end
                of[id] = o
            end
            line = r:readLine()
        end
        r:close()
    end)
    self.ovApp = { at = t, key = self:orderKey(), of = found and of or nil }
    return self.ovApp.of
end

--- Which enabled mods replace whose files (same path, the later one in load order wins), worked out a few
--- mods a frame: id -> { wins = { id -> n }, loses = { id -> n } }, and whether it's done.
function PZMMMods:overlaps()
    local app = self:appOverlaps()
    if app then return app, true end
    local key = self:orderKey()
    if not self.ov or self.ov.key ~= key then
        local todo = {}
        for i, id in ipairs(self.order or {}) do todo[i] = id end
        self.ov = { key = key, todo = todo, files = {}, of = {} }
    end
    local ov = self.ov
    for _ = 1, 3 do
        local id = table.remove(ov.todo, 1)
        if not id then break end
        for _, dir in ipairs(self.model.mods[id] and K.OVERLAP_DIRS or {}) do
            pcall(function()
                local a = listFilesInModDirectory(id, dir)
                for i = 0, (a and a:size() or 0) - 1 do
                    local f = dir .. "/" .. tostring(a:get(i)):gsub("\\", "/"):match("([^/]+)$"):lower()
                    ov.files[f] = ov.files[f] or {}
                    table.insert(ov.files[f], id)
                end
            end)
        end
        if #ov.todo == 0 then
            for _, ids in pairs(ov.files) do
                for i = 2, #ids do
                    for j = 1, i - 1 do
                        local w, l = ids[i], ids[j]
                        ov.of[w] = ov.of[w] or { wins = {}, loses = {} }
                        ov.of[l] = ov.of[l] or { wins = {}, loses = {} }
                        ov.of[w].wins[l] = (ov.of[w].wins[l] or 0) + 1
                        ov.of[l].loses[w] = (ov.of[l].loses[w] or 0) + 1
                    end
                end
            end
        end
    end
    return ov.of, #ov.todo == 0
end

--- A mod's load rules, for code above this block (it can't see the local).
function PZMMMods:rulesOf(d) return rules(d) end

--- Each enabled mod's first load order problem (id -> { why, fixable }), how many, and how many Fix can fix.
function PZMMMods:orderProblems()
    local pos, mods = {}, self.model.mods
    for i, id in ipairs(self.order or {}) do pos[id] = i end
    local function name(id) return mods[id] and mods[id].name or id end
    local out, n, fixable = {}, 0, 0
    for i, id in ipairs(self.order or {}) do
        local d = mods[id]
        local p
        if d then
            local r = rules(d)
            for _, q in ipairs(r.requires) do
                if not mods[q] then
                    p = { "Needs " .. q .. ", which isn't installed.", false }
                elseif not pos[q] then
                    p = { "Needs " .. name(q) .. ", which is off.", mods[q].isAvailable ~= false }
                elseif pos[q] > i then
                    p = { "Needs " .. name(q) .. " to load first.", true }
                end
                if p then break end
            end
            if not p then
                for _, a in ipairs(r.after) do
                    if pos[a] and pos[a] > i then p = { "Should load after " .. name(a) .. ".", true }; break end
                end
            end
            if not p then
                for _, b in ipairs(r.before) do
                    if pos[b] and pos[b] < i then p = { "Should load before " .. name(b) .. ".", true }; break end
                end
            end
        else
            p = { "Not installed: the game skips it.", false }
        end
        if p then
            out[id], n = p, n + 1
            if p[2] then fixable = fixable + 1 end
        end
    end
    return out, n, fixable
end

--- Find the broken mod (the app's bisect, order.ts bisectRound): the suspects are the enabled mods (but not the
--- frameworks others need: they stay on); each round turns on the first half of them with what they need, and
--- the rest off, and saves it. You start the game, see whether the problem happens, and answer here.
--- The enabled mods `order` as dependency-closed groups: a mod and every enabled mod it needs (or that needs it) are one
--- group, so a search never turns a requirement off under a mod that stays on. Members keep load order; groups follow
--- their first member. `reqs(id)` lists a mod's requirements (off or missing ones add nothing). Checked against
--- tests/fixtures/bisect-groups.json, as the app's grouping is.
function K.bisectGroups(order, reqs)
    local pos, parent = {}, {}
    for i, id in ipairs(order) do pos[id], parent[id] = i, id end
    local function find(a)
        while parent[a] ~= a do
            parent[a] = parent[parent[a]]
            a = parent[a]
        end
        return a
    end
    -- a requirement that's off is followed through to the enabled mods it needs: turning a group on turns it
    -- and them on, so they must be in that group
    for _, id in ipairs(order) do
        local seen = {}
        local function link(q)
            if seen[q] then return end
            seen[q] = true
            if pos[q] then
                local a, c = find(id), find(q)
                if a ~= c then parent[c] = a end
            else
                for _, r in ipairs(reqs(q) or {}) do link(r) end
            end
        end
        for _, q in ipairs(reqs(id) or {}) do link(q) end
    end
    local groups, at = {}, {}
    for _, id in ipairs(order) do
        local r = find(id)
        if not at[r] then
            groups[#groups + 1] = {}
            at[r] = #groups
        end
        local g = groups[at[r]]
        g[#g + 1] = id
    end
    return groups
end

--- A leave-out file: "<server>\t<id>;<id>" lines -> { [lower server] = { name = as written, ids = { id -> true } } }.
--- The app's is leaveout.txt (it owns the list); the game keeps leaveout_game.txt only while the app isn't there.
function K.readLeaveOut(file)
    local out = {}
    pcall(function()
        local r = getFileReader(file, false)
        if not r then return end
        local line = r:readLine()
        while line do
            local name, list = line:match("^([^\t]+)\t(.*)$")
            if name then
                local e = { name = name, ids = {} }
                for id in list:gmatch("[^;]+") do e.ids[id] = true end
                out[name:lower()] = e
            end
            line = r:readLine()
        end
        r:close()
    end)
    return out
end

--- What a server gets, as the app's lines_for: the enabled mods in `order` not in `left`; the Workshop items of the
--- kept ones (an item stays while any kept mod comes from it); `maps` (load order) without the maps only left-out
--- mods bring (`mapsOf(id)`: a mod's map folders). Returns ids, items, maps, and what was dropped.
function K.serverLists(order, left, wsOf2, maps, mapsOf)
    local ids, items, seenItem, dropped = {}, {}, {}, {}
    local keptMaps, leftMaps = {}, {}
    for _, id in ipairs(order) do
        if left[id] then
            dropped[#dropped + 1] = id
            for _, m in ipairs(mapsOf(id) or {}) do leftMaps[m] = true end
        else
            ids[#ids + 1] = id
            local ws = wsOf2(id)
            if ws and ws ~= "" and ws ~= "0" and not seenItem[ws] then
                seenItem[ws] = true
                items[#items + 1] = ws
            end
            for _, m in ipairs(mapsOf(id) or {}) do keptMaps[m] = true end
        end
    end
    -- Map=: from the kept mods' maps (with the ones no mod brings, like the game's own); none kept but some left
    -- out: just the game's own map, so theirs go; none at all: {} (the server's Map= stays as it is)
    local outMaps, keptAny, leftAny = {}, next(keptMaps) ~= nil, next(leftMaps) ~= nil
    if keptAny then
        for _, m in ipairs(maps) do
            if keptMaps[m] or not leftMaps[m] then outMaps[#outMaps + 1] = m end
        end
    elseif leftAny then
        outMaps = { K.BASE_MAP }
    end
    return ids, items, outMaps, dropped
end

--- Kept mods that need a left-out one (the app's hosting.check.needsLeftOut): { { id, needs } }.
function K.needsLeftOut(order, left, reqs)
    local out = {}
    for _, id in ipairs(order) do
        if not left[id] then
            for _, q in ipairs(reqs(id) or {}) do
                if left[q] then out[#out + 1] = { id, q } end
            end
        end
    end
    return out
end

--- Whether a mod's requirements, followed through installed mods, reach Project Viewpoint or ZombieBuddy (the app's
--- "client-side add-on?" label; not Viewpoint or ZombieBuddy themselves).
function K.clientSide(id, reqs)
    if id == "Viewpoint" or id == "ZombieBuddy" then return false end
    local seen, hit = {}, false
    local function walk(m)
        if hit or seen[m] then return end
        seen[m] = true
        for _, q in ipairs(reqs(m) or {}) do
            if q == "Viewpoint" or q == "ZombieBuddy" then hit = true return end
            walk(q)
        end
    end
    walk(id)
    return hit
end

--- The suspects' order, as the app's bisectOrder: the groups holding a mod in `first` lead (`lead` = how many), then
--- the rest newest first by their newest member's first-seen date (`seen`: id -> secs, firstseen.txt); ties, and
--- no dates at all, keep load order. Returns { groups, lead }.
function K.bisectOrder(groups, seen, first)
    local want = {}
    for _, id in ipairs(first or {}) do want[id] = true end
    local lead, rest = {}, {}
    for _, g in ipairs(groups) do
        local hit = false
        for _, id in ipairs(g) do if want[id] then hit = true end end
        if hit then lead[#lead + 1] = g else rest[#rest + 1] = { g = g, i = #rest + 1 } end
    end
    for _, r in ipairs(rest) do
        r.at = 0
        for _, id in ipairs(r.g) do r.at = math.max(r.at, seen and seen[id] or 0) end
    end
    -- stable: equal dates keep their load order
    table.sort(rest, function(x, y) if x.at ~= y.at then return x.at > y.at end return x.i < y.i end)
    local n = #lead
    for _, r in ipairs(rest) do lead[#lead + 1] = r.g end
    return { groups = lead, lead = n }
end

--- One test, as the app's bisectRound: every mod outside the suspect groups stays on (`original` order), plus the
--- first `count` suspect groups; `alone`: only the suspect groups and the mods kept as they are (`keep`: on mods this
--- build can't load). Returns the set of mods on and the groups under test.
function K.bisectRound(original, suspects, count, alone, keep)
    local suspect, on, testing = {}, {}, {}
    for _, g in ipairs(suspects) do for _, id in ipairs(g) do suspect[id] = true end end
    for _, id in ipairs(original) do
        if (keep and keep[id]) or (not alone and not suspect[id]) then on[id] = true end
    end
    if alone then count = #suspects end
    for i = 1, math.min(count, #suspects) do
        testing[i] = suspects[i]
        for _, id in ipairs(suspects[i]) do on[id] = true end
    end
    return on, testing
end

--- After a test, as the app's bisectNext: still broken narrows to the groups tested, fixed to the others. One group
--- left because it was tested and still broken is the answer; one left only because the others were cleared is
--- tested alone next (`alone`), and if it alone is fine it breaks only together with mods in another group.
--- Returns { found = ids } | { together = ids } | { none = true } | { suspects = groups, alone = bool }.
function K.bisectNext(b, still)
    if b.alone then return still and { found = b.suspects[1] } or { together = b.suspects[1] } end
    local tested = {}
    for _, g in ipairs(b.testing) do tested[g[1]] = true end
    local next = {}
    for _, g in ipairs(b.suspects) do
        if (tested[g[1]] == true) == still then next[#next + 1] = g end
    end
    if #next == 0 then return { none = true } end
    if #next == 1 and still then return { found = next[1] } end
    return { suspects = next, alone = #next == 1 }
end

function PZMMMods:bisectApply()
    local b = self.bisect
    -- round 1 has every suspect off (still broken there: it isn't these mods); round 2 the lead groups (the mods it
    -- was asked to start with) when there are some but not all; then halves (as the app's bisectApply)
    local count = b.round == 1 and 0 or (b.round == 2 and b.lead and b.lead > 0 and b.lead < #b.suspects) and b.lead or math.ceil(#b.suspects / 2)
    local on, testing = K.bisectRound(b.original, b.suspects, count, b.alone, b.keep)
    b.testing = testing
    local list = {}
    for _, id in ipairs(b.original) do if on[id] then list[#list + 1] = id end end
    writeBisect(b)
    self:setOrder(list)
    -- the game's Accept drops the maps of mods that are off: put the order back each round
    if b.maps then self:setMapOrder(b.maps) end
    self.findings = nil
    self:say("Test list saved: start the game, see whether the problem happens, then answer here.")
    self:acceptNow()
end

--- `first`: mods to start with (their groups lead, as the app's bisectStart(first)).
function PZMMMods:bisectStart(first)
    if self.bisect then return end
    if self:changes(true) > 0 then return self:say("Accept or discard your changes first: the search saves its test lists as it goes.") end
    local mods = self.model.mods
    -- every enabled mod, in load order, comes back after; mods this build can't load aren't suspects: they stay as
    -- they are in every round (as the app's)
    local original, had, keep, loads = {}, {}, {}, {}
    for _, id in ipairs(self:activeOrder()) do
        local d = mods[id]
        if d then
            original[#original + 1] = id
            had[id] = true
            if d.isAvailable == false then keep[id] = true else loads[#loads + 1] = id end
        end
    end
    -- frameworks are suspects too, always in the same group as the mods that need them
    -- (asked for mods that are off too, to follow a chain through them; a mod that isn't installed needs nothing here)
    local suspects = K.bisectGroups(loads, function(id)
        local d, out = mods[id], {}
        if not d then return out end
        for q in pairs(d.requireMods or {}) do out[#out + 1] = q end
        for _, q in ipairs(rules(d).requires) do out[#out + 1] = q end
        return out
    end)
    if #loads < 2 then return self:say(T("bisect_needs_two", "Enable at least two mods to search for a broken one.")) end
    if #suspects < 2 then
        local ids = {}
        for _, g in ipairs(suspects) do for _, id in ipairs(g) do ids[#ids + 1] = id end end
        return self:say(T("bisect_one_group", "Your enabled mods all need each other (%1), so the search can't split them into groups to test: turn some off by hand to narrow it down.", table.concat(ids, ", ")))
    end
    -- the groups holding a mod to start with go first, then the newest arrivals first (firstseen.txt, from the app)
    suspects = K.bisectOrder(suspects, self.firstSeen, first)
    local maps = {}
    pcall(function()
        local mo = self.model:getActiveMods():getMapOrder()
        for i = 0, mo:size() - 1 do maps[#maps + 1] = mo:get(i) end
    end)
    self.bisect = { original = original, had = had, keep = keep, suspects = suspects.groups, testing = {}, round = 1, maps = maps, lead = suspects.lead }
    self:bisectApply()
end

--- Answer a test: does the problem still happen (the groups under test hold the broken mod) or is it gone.
function PZMMMods:bisectAnswer(still)
    local b = self.bisect
    if not (b and b.suspects) then return end
    local n = K.bisectNext(b, still)
    if n.found then return self:bisectFinish(table.concat(n.found, "|")) end
    if n.together then return self:bisectFinish("~" .. table.concat(n.together, "|")) end
    if n.none then return self:bisectFinish("") end
    b.suspects, b.round, b.alone = n.suspects, b.round + 1, n.alone or nil
    self:bisectApply()
end

--- Put the mod list back as it was before the search, keep what it found (nil: stopped) and save.
function PZMMMods:bisectFinish(found)
    local b = self.bisect
    if not (b and b.original) then return end
    local list = {}
    for _, id in ipairs(b.original) do if self.model.mods[id] then list[#list + 1] = id end end
    self.bisect = found and { result = found } or nil
    writeBisect(self.bisect)
    self:setOrder(list)
    if b.maps then self:setMapOrder(b.maps) end
    self.findings = nil
    self:acceptNow()
end

--- The panel at the top of Check setup: start the search, or where it stands, or what it found.
function PZMMMods:drawBisect(x, y, w, mx, my)
    local b = self.bisect
    local function buttons(list)
        local bx = x
        for _, a in ipairs(list) do
            local bw = width(a[1]) + 28
            if bx > x and bx + bw > x + w then bx, y = x, y + BAR + 6 end
            local _, _, over = flatButton(self, a[1], bx, y, mx, my, a[2])
            if over and a[4] then tip(self, a[4]) end
            self.hits[#self.hits + 1] = { kind = "fn", fn = a[3], x = bx, y = y, w = bw, h = BAR }
            bx = bx + bw + 8
        end
        y = y + BAR + 14
    end
    if not b then
        buttons({ { T("findings_find", "Find the broken mod"), nil, function() self:bisectStart() end,
            "Find the broken mod\nTurns off half of your mods at a time, keeping mods that need each other together, and asks whether the problem still happens, until one mod or group is left. Your mod list is restored after." } })
        return y
    end
    if b.result ~= nil then
        local together = b.result:sub(1, 1) == "~"
        local members = {}
        for id in b.result:gsub("^~", ""):gmatch("[^|]+") do members[#members + 1] = id end
        local d = #members == 1 and self.model.mods[members[1]] or nil -- (Show it, found or together)
        local names = {}
        for _, id in ipairs(members) do names[#names + 1] = self.model.mods[id] and self.model.mods[id].name or id end
        -- the app's texts (app/src/lib/locales log.bisect*), word for word in every language
        local msg = together and T("bisect_together", "%1 breaks it only together with mods in another group: turning it off fixes your list, but it's fine on its own. Your mod list is restored.", table.concat(names, ", "))
            or #members == 1 and T("bisect_found", "Found it: %1 causes the problem. Your mod list is restored.", names[1])
            or #members > 1 and T("bisect_found_group", "It's in this group: %1. These mods need each other, so they were tested together: turn off the ones nothing else needs, one at a time, to find which. Your mod list is restored.", table.concat(names, ", "))
            or T("bisect_none", "The problem still happens with every suspect off, so it isn't one of these mods. Your mod list is restored.")
        local y0 = y
        for _, l in ipairs(wrap(msg, w - 12)) do
            text(self, l, x + 12, y, C.text)
            y = y + H_SMALL + 2
        end
        rect(self, x, y0, 2, y - y0, b.result ~= "" and C.danger or C.muted, 1)
        y = y + 6
        local acts = {}
        if d then acts[#acts + 1] = { "Show it", "primary", function() self.selected, self.checkOpen = d, false end } end
        acts[#acts + 1] = { "Dismiss", "quiet", function() self.bisect = nil; writeBisect(nil) end }
        buttons(acts)
        return y
    end
    local y0 = y
    bold(self, "Finding the broken mod, round " .. b.round .. ":", x + 12, y, C.text)
    y = y + H_SMALL + 4
    local n, t, nm = #b.suspects, #b.testing, 0
    for _, g in ipairs(b.suspects) do nm = nm + #g end
    local msg = nm .. (nm == 1 and " suspect mod" or " suspect mods") .. " left in " .. n .. (n == 1 and " group" or " groups") .. ", " .. t .. (t == 1 and " group" or " groups")
        .. " on for this test. Mods that need each other are tested together. Start the game, check whether the problem happens, then come back here."
    for _, l in ipairs(wrap(msg, w - 12)) do
        text(self, l, x + 12, y, C.text)
        y = y + H_SMALL + 2
    end
    rect(self, x, y0, 2, y - y0, C.accentText, 1)
    y = y + 6
    buttons({
        { "The problem still happens", "primary", function() self:bisectAnswer(true) end },
        { "The problem is gone", nil, function() self:bisectAnswer(false) end },
        { "Stop and restore my list", "quiet", function() self:bisectFinish(nil) end },
    })
    return y
end

--- Turn on what enabled mods need (and what that needs), when it's installed and loads on this build.
function PZMMMods:enableRequirements()
    local mods, changed = self.model.mods, true
    while changed do
        changed = false
        for _, d in pairs(mods) do
            if d.isActive then
                for _, q in ipairs(rules(d).requires) do
                    local dep = mods[q]
                    if dep and not dep.isActive and dep.isAvailable ~= false then
                        self.model:setModActive(q, true)
                        dep.isActive = true
                        changed = true
                    end
                end
            end
        end
    end
end

--- An enabled mod `d` can't go on beside (it lists it as incompatible, or it lists `d`), or nil.
function PZMMMods:clashesOn(d)
    local mods = self.model.mods
    for id in pairs(d.incompatibleWith or {}) do
        local o = mods[id]
        if o and o ~= d and o.isActive then return o end
    end
    for _, o in pairs(mods) do
        if o ~= d and o.isActive and o.incompatibleWith and o.incompatibleWith[d.modId] then return o end
    end
    return nil
end

--- What to say after mods were skipped by setMany.
function PZMMMods:skipNote(n)
    return n .. (n == 1 and " mod" or " mods") .. " skipped: can't load on this build, or clashes with a mod that's on."
end

--- Mass on/off without forceActivateMods per mod (each one re-checks the whole list and froze the screen):
--- the model's setModActive for each, then what the new ones need, then one refresh. Mods that can't load,
--- or clash with one that's on, are skipped (the first of two clashing ones wins). Returns how many.
function PZMMMods:setMany(ons, offs)
    for _, d in ipairs(offs) do
        self.model:setModActive(d.modId, false)
        d.isActive = false
    end
    local skipped, turned = 0, 0
    for _, d in ipairs(ons) do
        if not d.isAvailable or self:clashesOn(d) then
            skipped = skipped + 1
        else
            self.model:setModActive(d.modId, true)
            d.isActive = true
            turned = turned + 1
        end
    end
    if turned > 0 then self:enableRequirements() end
    self.model:refreshMods()
    return skipped
end

--- The enabled mods, in load order, as the game has them now.
function PZMMMods:activeOrder()
    local out = {}
    pcall(function()
        local a = self.model:getActiveMods():getMods()
        for i = 0, a:size() - 1 do out[#out + 1] = a:get(i) end
    end)
    return out
end

--- Make `list` the load order (saved with Accept, as the game's own load order screen does).
function PZMMMods:setOrder(list, said)
    pcall(function()
        local a = self.model:getActiveMods():getMods()
        a:clear()
        for _, id in ipairs(list) do a:add(id) end
    end)
    self.model:refreshMods()
    self.diffAt = nil
    if said then self:say(said) end
end

--- Fix load order (the app's): turn on missing requirements, then reorder so every rule holds.
function PZMMMods:fixOrder()
    local snap = self:snapshot()
    self:enableRequirements()
    self:setOrder(sortLoadOrder(self:activeOrder(), self.model.mods), "Load order fixed: every requirement and load rule holds.")
    self:rememberIfChanged(snap)
end

--- Run every safe fix at once (one Undo step).
function PZMMMods:fixSafely(ons, order)
    local snap = self:snapshot()
    local skipped = self:setMany(ons, {})
    if order then
        self:enableRequirements()
        self:setOrder(sortLoadOrder(self:activeOrder(), self.model.mods))
    end
    self:rememberIfChanged(snap)
    self.findings = nil
    self:say("Fixed what only needed turning on or reordering." .. (skipped > 0 and (" " .. self:skipNote(skipped)) or ""))
end

--- Autosort (the app's): requirements on, frameworks first and translations last (by category), then every
--- rule satisfied, keeping your order everywhere else.
function PZMMMods:autosortOrder()
    local snap = self:snapshot()
    self:enableRequirements()
    self:countNeeded()
    local cats = readCategories() or { of = {} }
    local first = (cats.order or K.CATEGORIES)[1]
    local function tier(id)
        local d = self.model.mods[id]
        local c = cats.of[id] or (d and self:guessCategory(d))
        return c == first and 0 or c == "Translations" and 2 or 1
    end
    -- the rules file's loadFirst / loadLast: "on" is the whole list, "category" within the tier (the app's autosort)
    local function edge(id, scope)
        local f = FILE_RULES[id]
        if f and f.first == scope then return 0 end
        if f and f.last == scope then return 2 end
        return 1
    end
    local function rank(id) return edge(id, "on") * 100 + tier(id) * 10 + edge(id, "category") end
    local list = self:activeOrder()
    local at = {}
    for i, id in ipairs(list) do at[id] = i end
    table.sort(list, function(a, b)
        local ta, tb = rank(a), rank(b)
        if ta ~= tb then return ta < tb end
        return at[a] < at[b]
    end)
    self:setOrder(pullUp(list, self.model.mods), "Autosorted: frameworks first, translations last, every rule holding.")
    self:rememberIfChanged(snap)
end

--- A to Z (the app's): by name, with requirements and load rules still first.
function PZMMMods:sortOrderAz()
    local snap = self:snapshot()
    local list, mods = self:activeOrder(), self.model.mods
    local function rk(id)
        local f = FILE_RULES[id]
        return f and (f.first == "on" and 0 or f.last == "on" and 2) or 1
    end
    table.sort(list, function(a, b)
        local da, db = mods[a], mods[b]
        if not (da and db) then return da ~= nil end
        if rk(a) ~= rk(b) then return rk(a) < rk(b) end
        return byName(da, db)
    end)
    self:setOrder(pullUp(list, mods), "Load order sorted A to Z.")
    self:rememberIfChanged(snap)
end

--- The library's lines to edit: the edits PZMM hasn't acked yet, else as they are on disk now (the app may
--- have written it since).
function PZMMMods:libLines()
    self.lib = self:readLib() or self.lib
    local lines = {}
    for i, l in ipairs(self.lib and self.lib.lines or {}) do lines[i] = l end
    return lines
end

--- The library as shown: with the edits PZMM hasn't acked yet.
function PZMMMods:readLib()
    return self.overlay and parseLibrary(self.overlay) or readLibrary()
end

--- Whether PZMM is running now (its app.txt is fresh), read again at most every 2 s.
function PZMMMods:appUp()
    local t = getTimestampMs()
    if not self.appAt or t - self.appAt > 2000 then
        local was = self.app
        self.appAt = t
        -- a read that failed keeps the old value; the file really gone is nil
        local read, gone = readApp()
        if read or gone then self.app = read end
        applyTheme(self.app)
        local editing = false
        for _ in pairs(self.afterAck or {}) do editing = true end
        if self.app and not editing then
            self.tags = readTags()
            self.installed, self.hasInstalled = readInstalled()
            if self.filterTag then self:updateView() end
        end
        local a = self.app
        if was and a then
            if was.running ~= a.running then self:addLog(a.running and "PZMM is running: what you change here reaches your profile." or "PZMM closed: changes here wait for it to start again.", "info", "sync") end
            if a.running and was.profile and a.profile and was.profile ~= a.profile then
                self:addLog("PZMM switched to the profile " .. a.profile .. ": its mods are on here now.", "info", "sync")
                self.undos, self.redos = {}, {}
                -- the app rewrote default.txt: read it again; that is the new baseline (as open() does)
                self.selected = nil
                pcall(function() self.model:reloadMods() end)
                self.defRev, self.defChanged = a.default_rev, nil
                -- the game's list still has the old profile's mods: Accept would write them into the new one
                self:takeAppMods()
            end
        end
        -- default.txt changed while this screen is open, and not by it (open() and its first seconds baseline it)
        if a and a.default_rev then
            -- while our own edits wait for PZMM's ack, and for a few seconds after it, default.txt changing is those edits
            local mine = self:waiting()
            local ours = mine or (self.rebaseUntil and t < self.rebaseUntil)
            if not self.defRev or (self.openedAt and t - self.openedAt < 10000) or (ours and not mine) then
                self.defRev = a.default_rev
            elseif ours then
                -- (nothing to tell)
            elseif a.default_rev ~= self.defRev and not self.defChanged then
                self.defChanged = true
                self:say("PZMM changed your mods since you opened this screen: Use PZMM's mods takes them.")
            end
        end
        -- PZMM may have changed the library itself (Group by category, its own edits): show that
        if self.app and self.app.running and not self.overlay and not self.drag then
            local lib = readLibrary()
            if lib and table.concat(lib.lines, "\n") ~= table.concat(self.lib and self.lib.lines or {}, "\n") then
                if self.lib then
                    self:addLog("PZMM changed your sections.", "info", "sync")
                    self.undos, self.redos = {}, {}
                end
                self.lib = lib
                self:updateView()
            end
        end
    end
    return self.app ~= nil and self.app.running
end

--- Show the library's lines and keep them: with PZMM running the app writes library.txt (its request is on
--- the way), so they wait in memory until it acks; without it, they're written now. Returns whether it worked.
function PZMMMods:writeLibrary(lines)
    if self.app and self:appUp() then
        self.overlay = lines
        self.lib = parseLibrary(lines)
        self:updateView()
        return true
    end
    self.overlay = nil
    local ok = pcall(function()
        local w = getFileWriter("pzmm/library.txt", true, false)
        w:write(table.concat(lines, "\n") .. "\n")
        w:close()
    end)
    if not ok then
        self:say("Couldn't save your sections: Zomboid/Lua/pzmm/library.txt can't be written.")
        return false
    end
    self.lib = readLibrary()
    self:updateView()
    return true
end

--- A sorting change: with the app, PZMM makes it in the profile too (quietly); without it, said here.
function PZMMMods:toApp(line, said)
    self:appUp()
    if self.app then self:request(line, said, true) else self:say(said) end
    -- a request that couldn't be written won't be acked: what's shown is what's on disk
    if self.overlay and not self:waiting() then self:settle() end
end

--- Whether any request is still waiting for its ack.
function PZMMMods:waiting()
    for _ in pairs(self.pending) do return true end
    return false
end

--- Things waiting for PZMM to do a request first (a reset after its backup): run on its "ok" ack, said and
--- dropped on "no". Checked every second.
function PZMMMods:afterAcks()
    local t = getTimestampMs()
    if self.ackAt and t - self.ackAt < 1000 then return end
    self.ackAt = t
    -- a request or a job not acked in 3 minutes is given up, said as an error (never a silent wait)
    self.sentAt = self.sentAt or {}
    local gone = {}
    for id, said in pairs(self.sent) do
        if t - (self.sentAt[id] or t) > 180000 then gone[#gone + 1] = { id, said, false } end
        self.sentAt[id] = self.sentAt[id] or t
    end
    for id, job in pairs(self.afterAck) do
        job[3] = job[3] or t
        if t - job[3] > 180000 then gone[#gone + 1] = { id, job[2], true, job } end
    end
    for _, g in ipairs(gone) do
        self.sent[g[1]], self.sentAt[g[1]], self.afterAck[g[1]] = nil, nil, nil
        if self.kinds then self.kinds[g[1]] = nil end
        self:say(g[3] and (g[2] .. ": PZMM didn't answer in time.") or ("PZMM didn't answer (" .. tostring(g[2]) .. ")."), "error")
        if g[4] and g[4][4] then self:offerWithout(g[4], "PZMM didn't answer in time.") end
    end
    local waiting = false
    for _ in pairs(self.afterAck) do waiting = true end
    for _ in pairs(self.sent) do waiting = true end
    if not waiting then return end
    pcall(function()
        local r = getFileReader("pzmm/acks.txt", false)
        if not r then return end
        local line = r:readLine()
        while line do
            local id, ok, said = line:match("^([^\t]+)\t(%a+)\t?(.*)$")
            if id and self.sent[id] then
                if ok ~= "ok" then
                    self:say("PZMM didn't do it (" .. self.sent[id] .. "): " .. (said ~= "" and said or "no reason given."), "error")
                else
                    local kind = self.kinds and self.kinds[id]
                    if kind == "install" then self:say("Installed: Rescan to see the new mods.")
                    elseif kind == "restore" then self.saves = nil end
                end
                if self.kinds then self.kinds[id] = nil end
                self.sent[id] = nil
            end
            local job = id and self.afterAck[id]
            if job then
                self.afterAck[id] = nil
                if ok == "ok" then job[1]() else
                    if job[6] then job[6]() end
                    self:say(job[5] or (job[2] .. ": " .. (said ~= "" and said or "PZMM didn't do it.")))
                    if job[4] then self:offerWithout(job, said ~= "" and said or "PZMM didn't do it.") end
                end
            end
            line = r:readLine()
        end
        r:close()
    end)
end

--- A job whose backup didn't happen: offer job[4] = { label, fn } (go on without it).
function PZMMMods:offerWithout(job, why)
    self:ask({ title = job[2] .. ": " .. why, choices = { { job[4][1], job[4][2] } } })
end

--- The edits PZMM hasn't acked: settled once each request is acked (its library.txt has them, or says why
--- not), or written here if PZMM closed meanwhile (its next start does the requests). Checked every second.
function PZMMMods:settle()
    if not self.overlay and not self:waiting() then return end
    local t = getTimestampMs()
    if self.settleAt and t - self.settleAt < 1000 and self:waiting() then return end
    self.settleAt = t
    -- an id never acked (the app lost it) would pin the overlay: after 20 s it counts as acked, unless a
    -- running PZMM says it's busy with requests (app.txt busy=1)
    local busy = self.app and self.app.running and self.app.busy == "1"
    local stale = {}
    if not busy then
        for id, at in pairs(self.pending) do
            if t - at > 20000 then stale[#stale + 1] = id end
        end
    end
    for _, id in ipairs(stale) do self.pending[id] = nil end
    if #stale > 0 then self.rebaseUntil = t + 6000 end
    local why
    pcall(function()
        local r = getFileReader("pzmm/acks.txt", false)
        if not r then return end
        local line = r:readLine()
        while line do
            local id, ok, said = line:match("^([^\t]+)\t(%a+)\t?(.*)$")
            if id and self.pending[id] then
                self.pending[id] = nil
                if ok ~= "ok" and said ~= "" then why = said end
                -- PZMM did our edit: what it did to default.txt is no change of its own
                if ok == "ok" then self.rebaseUntil = getTimestampMs() + 6000 end
            end
            line = r:readLine()
        end
        r:close()
    end)
    if not self.overlay then return end -- (acks for edits with no overlay, like tags, are read above)
    local waiting = self:waiting()
    if waiting and self:appUp() then return end
    if waiting then
        -- PZMM closed before it acked: keep the edits on disk; its next start makes them in the profile
        local lines = self.overlay
        pcall(function()
            local w = getFileWriter("pzmm/library.txt", true, false)
            w:write(table.concat(lines, "\n") .. "\n")
            w:close()
        end)
    end
    self.overlay, self.pending = nil, {}
    self.lib = readLibrary()
    self:updateView()
    if why then self:say(why) end
end

--- Sections of your own without the app: the list as it's grouped now (Subscribed, On this PC, Can't
--- load), written in the app's format, ready to sort. With the app the profile's are there already.
function PZMMMods:ensureLibrary()
    if self.lib then return true end
    if self.app then
        self:say("Open PZMM once: it brings your profile's sections here to sort.")
        return false
    end
    -- one group per source (K.SOURCES), PZMM's Workshop copies included
    local lines, groups = { "# PZMM profile: In game" }, {}
    for i = 1, #K.SOURCES do groups[i] = {} end
    for _, d in pairs(self.model.mods) do table.insert(groups[source(d)], d) end
    for i, g in ipairs(groups) do
        if #g > 0 then
            table.sort(g, byName)
            lines[#lines + 1] = "# --- " .. K.SOURCES[i]
            for _, d in ipairs(g) do lines[#lines + 1] = (d.isActive and "+" or "-") .. d.modId end
        end
    end
    return self:writeLibrary(lines)
end

--- A name for a new or renamed section: trimmed, one line, not taken. nil (and why, in the footer) if not.
function PZMMMods:sectionNameOk(name, old)
    name = (name or ""):gsub("^%s+", ""):gsub("%s+$", "")
    if name == "" or #name > 60 or name:find("%c") then
        self:say("A section's name is one line of up to 60 characters.")
        return nil
    end
    for _, n in ipairs(self.lib and self.lib.sections or {}) do
        if n:lower() == name:lower() and n ~= old then
            self:say("There's a section called " .. n .. " already.")
            return nil
        end
    end
    return name
end

--- Remove section `g`: its mods stay where they are, outside any section (Other); an empty one just goes.
function PZMMMods:removeSection(g)
    if not self:ensureLibrary() then return end
    -- as the app does: its mods stay where they are, outside any section (Other); an empty one just goes
    local all, lines, done = self:libLines(), {}, false
    local empty = (self.lib.count[g] or 0) == 0
    for _, l in ipairs(all) do
        if not done and sectionName(l) == g then
            done = true
            if not empty then lines[#lines + 1] = "# ---" end
        else
            lines[#lines + 1] = l
        end
    end
    if done then self:remember() end
    if done and self:writeLibrary(lines) then self:toApp("section\tremove\t" .. g, g .. " removed" .. (empty and "." or ": its mods are under Other now.")) end
end

--- Rename section `g` (asks for the new name).
function PZMMMods:renameSection(g)
    self:prompt("Rename " .. g, g, "Rename", function(name)
        if not self:ensureLibrary() then return end
        name = self:sectionNameOk(name, g)
        if not name or name == g then return end
        local lines, done = self:libLines(), false
        for i, l in ipairs(lines) do
            if not done and sectionName(l) == g then lines[i], done = "# --- " .. name, true end
        end
        if not done then return end
        self:remember()
        self.collapsed[name] = self.collapsed[g]
        if self:writeLibrary(lines) then self:toApp("section\trename\t" .. g .. "\t" .. name, g .. " is now " .. name .. ".") end
    end, nil, 60)
end

--- A section's menu (right-click its head): a new one under it, rename, remove, fold.
function PZMMMods:sectionMenu(g)
    local items = {
        { "New section below", function()
            self:prompt("New section, under " .. g, "", "Add", function(name)
                if not self:ensureLibrary() then return end
                name = self:sectionNameOk(name)
                if not name then return end
                self:remember()
                local lines = self:libLines()
                local at = sectionEnd(lines, g)
                table.insert(lines, at, "# --- " .. name)
                if self:writeLibrary(lines) then self:toApp("section\tadd\t" .. name .. "\t" .. g, name .. " added.") end
            end, nil, 60)
        end },
    }
    if not self.lib or self.lib.header[g] then
        items[#items + 1] = { "Rename", function() self:renameSection(g) end }
        if g ~= "Other" then
            items[#items + 1] = { "Remove section", function() self:removeSection(g) end, danger = true }
        end
    end
    items[#items + 1] = false
    items[#items + 1] = { self.collapsed[g] and "Unfold" or "Fold", function()
        self.collapsed[g] = not self.collapsed[g]
        K.writeCollapsed(self.collapsed)
        self:updateView()
    end }
    self:menu(items)
end

--- Ask for a line of text: `fn(text)` runs on `label` (or Enter).
function PZMMMods:prompt(title, text0, label, fn, mask, max)
    PZMMOverlay.show({ ask = { title = title, input = text0, mask = mask, max = max, choices = { { label, fn } } } })
end

function PZMMMods:drawRow(e, x, y, w, mx, my)
    local d = e.mod
    if d.ghost then return self:drawGhostRow(e, x, y, w, mx, my) end
    round(self, x, y, w, ROW, 8, C.panel, nil, false, e.last)
    if not e.last then rect(self, x, y + ROW - 1, w, 1, C.line, 0.6) end
    local over = mx >= x and mx < x + w and my >= y and my < y + ROW
    local h = ROW - (e.last and 8 or 0)
    -- as the app: hovered, the accent with a 1px edge; selected, a little softer with a 2px white edge;
    -- both with the white marker over the card's left edge
    local picked = self.selected == d
    local lit = picked or self.picked[d.modId] or (over and not self.drag)
    if lit then
        local t, c, a = picked and 2 or 1, picked and C.onSelected or C.accent, picked and 1 or 0.7
        rect(self, x, y, w, h, C.accent, (picked or self.picked[d.modId]) and 0.32 or 0.34)
        rect(self, x, y, w, t, c, a)
        rect(self, x, y + h - t, w, t, c, a)
        rect(self, x, y, t, h, c, a)
        rect(self, x + w - t, y, t, h, c, a)
        if K.SEL_MARK then self:drawTextureScaled(K.SEL_MARK, x - 5, y + math.floor(ROW / 2) - 7, 10, 14, 1, 1, 1, 1) end
    end
    -- the grip is a promise to drag: not while a search or filter hides part of the list
    if not self:filtering() then handle(self, x + 12, y + ROW / 2 - 6) end
    local bx, by = x + 32, y + (ROW - BOX) / 2
    checkbox(self, bx, by, d.isActive, not d.isAvailable)
    local mid = y + (ROW - H_SMALL) / 2
    local n = d.isActive and self.place[d.modId]
    if n then text(self, tostring(n), x + 84 - width(tostring(n)), mid, lit and C.onSelected or C.accentText) end

    -- right: where it's from (click: its Workshop page), and an update waiting
    local right = x + w - 16
    local py = y + (ROW - H_SMALL - 4) / 2
    local src = source(d)
    local st = self:steamOf(d)
    local look = K.look(st)
    local pillHit
    local waiting = look and st.state == "NeedsUpdate"
    -- a problem (missing requirements, can't load, incompatible) stands in for where it's from
    local miss = d.isActive and missingReqs(d) or {}
    local ptxt, ptip
    if not d.isAvailable then
        ptxt, ptip = T("problem_cant_load", "Can't load"), "Can't load\nThis mod doesn't load on this build of the game."
    elseif d.isIncompatible then
        ptxt, ptip = T("problem_incompatible", "Incompatible"), "Incompatible\nAn enabled mod it is incompatible with is on."
    elseif #miss > 0 then
        ptxt, ptip = "Missing " .. #miss .. " required", "Missing requirements\n" .. table.concat(miss, ", ") .. "\nPick the mod to subscribe to what it needs."
    end
    if ptxt then
        local pc = lit and C.onSelected or C.danger
        local pw = width(ptxt) + 1 + (TB.alert and 18 or 0)
        local px = right - pw
        if TB.alert then self:drawTextureScaled(TB.alert, px, y + math.floor((ROW - 14) / 2), 14, 14, 1, pc.r, pc.g, pc.b) end
        bold(self, ptxt, px + (TB.alert and 18 or 0), mid, pc)
        if mx >= px and mx < right and my >= y and my < y + ROW then tip(self, ptip) end
        right = px
    elseif waiting then
        -- an update waiting stands in for where it's from
        local r0 = right
        right = pill(self, look[1], right, py, C.warn, false, lit, nil, TB.update)
        if mx >= right and mx < r0 and my >= y and my < y + ROW then tip(self, look[3]) end
    elseif src == 1 or src == 4 then
        local r0 = right
        local lbl = K.SOURCES[src]
        local hot = mx >= right - (width(lbl) + 30) and mx < right and my >= py and my < py + H_SMALL + 4
        right = pill(self, lbl, right, py, C.ok, true, lit, nil, nil, hot)
        local ws = wsOf(d)
        if hot then tip(self, src == 4 and "Workshop page\nPZMM's copy of a Workshop item. Opens it in Steam." or "Workshop page\nOpens it in Steam.") end
        pillHit = { kind = "fn", fn = function() self:openWorkshop(ws) end, x = right, y = py, w = r0 - right, h = H_SMALL + 4 }
    elseif src == 2 then
        right = pill(self, "Local", right, py, C.muted, false, lit, nil, K.FOLDER)
    end

    -- shared files with another mod: the layers mark, accent where its files win, muted where they lose, white when lit
    local ovl = d.isActive and self.ovOf and self.ovOf[d.modId]
    local ox = right - 24
    if ovl and TB.overlap then
        local wins = false
        for _ in pairs(ovl.wins) do wins = true; break end
        local oc = lit and C.onSelected or (wins and C.accentText or C.muted)
        self:drawTextureScaled(TB.overlap, ox + 4, y + math.floor((ROW - 16) / 2), 16, 16, 1, oc.r, oc.g, oc.b)
        if mx >= ox and mx < ox + 24 and my >= y and my < y + ROW then
            local function names(set)
                local out = {}
                for id in pairs(set) do out[#out + 1] = (self.model.mods[id] and self.model.mods[id].name) or id end
                table.sort(out)
                return table.concat(out, ", ")
            end
            local t = "Shared files"
            local won = names(ovl.wins)
            local lost = names(ovl.loses)
            if won ~= "" then t = t .. "\nOverrides files from " .. won end
            if lost ~= "" then t = t .. "\nSome files are overridden by " .. lost end
            tip(self, t)
        end
    end
    if ovl or w >= 760 then right = ox - 8 end
    -- wide enough: its size and last change (Steam's, for Workshop mods)
    if w >= 760 then
        local stw = self:steamOf(d)
        local when = ""
        if stw and (stw.updated or 0) > 0 then
            local okd, ds = pcall(os.date, "%d %b %Y", K.LT(stw.updated))
            if okd then when = ds end
        end
        local sz = ""
        if stw and (stw.size or 0) > 0 then
            sz = stw.size >= 1048576 and ((math.floor(stw.size / 104857.6) / 10) .. " MB") or (math.floor(stw.size / 1024) .. " KB")
        end
        local c1 = lit and C.onSelected or C.muted
        right = right - 96
        if when ~= "" then text(self, when, right, mid, c1) end
        right = right - 76
        if sz ~= "" then text(self, sz, right, mid, c1) end
        if mx >= right and mx < right + 172 and my >= y and my < y + ROW and (sz ~= "" or when ~= "") then tip(self, "Size and last change\nSteam's numbers for this Workshop item.") end
        right = right - 8
    end
    -- the star of a favourite, the name (bold), mod ID, then chips; cut short before the pills
    local tx = x + 98
    local c = lit and C.onSelected or d.isActive and C.text or C.muted
    local room = right - 16 - tx
    if d.favorite and K.STAR_ON then
        local sc = lit and C.onSelected or C.accentText
        self:drawTextureScaled(K.STAR_ON, tx, y + math.floor((ROW - 12) / 2), 12, 12, 1, sc.r, sc.g, sc.b)
        tx, room = tx + 18, room - 18
    end
    local name = fit(d.name, room)
    bold(self, name, tx, mid, c)
    tx = tx + width(name) + 10
    if SETTINGS.ids and tx + width(d.modId) < right - 16 then
        text(self, d.modId, tx, mid, lit and copy(C.onSelected, 0.8) or C.muted)
        tx = tx + width(d.modId) + 10
    end
    local cy = y + (ROW - H_SMALL - 2) / 2
    -- New (installed since you last played), what kind of mod it is, its language when that filter is on
    if self:isNew(d) and tx + 60 < right then
        local cx = tx
        tx = chip(self, T("badge_new", "New"), tx, cy, C.accentText, lit, C.accent, 0.32) + 6
        if mx >= cx and mx < tx and my >= y and my < y + ROW then tip(self, "New\nInstalled since you last played.") end
    end
    for _, k in ipairs(self:kindsOf(d)) do
        if tx + width(k) + 30 < right then tx = chip(self, k, tx, cy, C.muted, lit, C.line) + 6 end
    end
    if self.langOnly and d.hasLang and tx + 50 < right then tx = chip(self, self:langName(), tx, cy, nil, lit) + 6 end
    local need = self.needed[d.modId]
    if need and tx + 90 < right then tx = chip(self, "Needed by " .. need, tx, cy, nil, lit) + 6 end
    for _, tg in ipairs((self.tags and self.tags[d.modId]) or {}) do
        if tx + width(tg) + 30 < right then tx = tagChip(self, tg, tx, cy, lit) + 6 end
    end
    local bt = d.isAvailable and self:buildTag(d)
    if bt and tx + 90 < right then
        local cx = tx
        tx = chip(self, bt, tx, cy, C.danger, lit) + 6
        if mx >= cx and mx < tx and my >= y and my < y + ROW then
            tip(self, bt .. "\n" .. (bt == "B41 only" and "It has no Build 42 folder: it was made for Build 41 and may not work on this build." or "It only has a Build 42 folder, which this build can't use."))
        end
    end
    -- its load order problem (what the Problems tab lists it for): needs a mod that's off, must load after another
    local prob = self.oprob and self.oprob[d.modId]
    if prob and #miss == 0 and tx + 150 < right then
        local cx = tx
        local mc = lit and C.onSelected or C.danger
        if TB.alert then
            self:drawTextureScaled(TB.alert, tx, y + math.floor((ROW - 14) / 2), 14, 14, 1, mc.r, mc.g, mc.b)
            tx = tx + 18
        end
        local ptext = fit((prob[1]:gsub("%.$", "")), right - 16 - tx)
        bold(self, ptext, tx, mid, mc)
        tx = tx + width(ptext) + 7
        if mx >= cx and mx < tx and my >= y and my < y + ROW then tip(self, "Load order problem\n" .. prob[1] .. (prob[2] and "\nFix in the load order panel puts it right." or "")) end
    end
    if look and not waiting and tx + 110 < right then
        local cx = tx
        tx = chip(self, look[1], tx, cy, C[look[2]], lit) + 6
        if mx >= cx and mx < tx and my >= y and my < y + ROW then tip(self, look[3]) end
    end
    if mx >= bx - 6 and mx < bx + BOX + 6 and my >= by - 6 and my < by + BOX + 6 then
        tip(self, not d.isAvailable and "Can't load\nThis mod doesn't load on this build of the game." or d.isActive and "Disable" or "Enable")
    end
    if mx >= x + 6 and mx < x + 26 and my >= y and my < y + ROW and not self.drag then
        tip(self, self.app and "Drag to sort\nMove it within or between your profile's sections. PZMM saves it to the profile." or "Drag to sort\nMove it within or between sections. Right-click a section to add, rename or remove one.")
    end
    -- (hits are tried in order: the source pill's comes before the row's)
    self.hits[#self.hits + 1] = { kind = "drag", mod = d, x = x + 6, y = y, w = 20, h = ROW }
    self.hits[#self.hits + 1] = { kind = "box", mod = d, x = bx - 6, y = by - 6, w = BOX + 12, h = BOX + 12 }
    if pillHit then self.hits[#self.hits + 1] = pillHit end
    self.hits[#self.hits + 1] = { kind = "row", mod = d, x = x, y = y, w = w, h = ROW }
end

--- Whether mod `d` has the tag (lower case).
function PZMMMods:hasTag(d, low)
    for _, t in ipairs((self.tags and self.tags[d.modId]) or {}) do
        if t:lower() == low then return true end
    end
    return false
end

--- What kind of mod `d` is, as the app's (category.rs kinds_of): Framework, Patch, Map, UI, Translation. PZMM's
--- category stands in for Workshop tags; the rest is read from its name.
function PZMMMods:kindsOf(d)
    if not d.kindBits then
        if not self.neededAll then self:countNeeded() end
        self.cats = self.cats or readCategories() or { of = {} }
        local cat = self.cats.of[d.modId]
        local text = " " .. d.modId:lower() .. " " .. (d.name or ""):lower() .. " "
        local function says(words)
            for _, w in ipairs(words) do if text:find(w, 1, true) then return true end end
            return false
        end
        d.kindBits = {
            (self.neededAll[d.modId] or 0) >= 2 or cat == K.CATEGORIES[1] or says({ "framework", " lib ", "library", " api ", "_api ", " core " }),
            says({ "patch", "compat", " fix ", "fixes", "hotfix", " addon", "add-on" }),
            d.category == "map" or cat == "Maps",
            cat == "Interface" or says({ " ui ", " ui)", "(ui", " hud", "interface", "menu" }),
            cat == "Translations" or says({ "translat", "tradu", "localiz", "locali" }),
        }
    end
    local b = d.kindBits
    local scan = self.mapScanState
    local out = {}
    if b[1] then out[#out + 1] = T("kind_framework", "Framework") end
    if b[2] then out[#out + 1] = T("kind_patch", "Patch") end
    if b[3] or (scan and scan.modOf[d.modId]) then out[#out + 1] = T("kind_map", "Map") end
    if b[4] then out[#out + 1] = T("kind_ui", "UI") end
    if b[5] then out[#out + 1] = T("kind_translation", "Translation") end
    return out
end

--- The kinds a mod can be, as kindsOf names them.
function K.kindNames() return { T("kind_framework", "Framework"), T("kind_patch", "Patch"), T("kind_map", "Map"), T("kind_ui", "UI"), T("kind_translation", "Translation") } end
--- Whether `d` is of kind `name`.
function K.hasKind(ui, d, name)
    for _, k in ipairs(ui:kindsOf(d)) do if k == name then return true end end
    return false
end

--- Installed since you last played: PZMM's installed.txt when it has the mod, else Steam's creation time.
function PZMMMods:isNew(d)
    local since = self.lastPlayed
    if not since then return false end
    local at = self.installed and self.installed[d.modId]
    if at then return at > since end
    local st = self:steamOf(d)
    return st ~= nil and (st.created or 0) > since
end

--- Replace the tags of mod `d` with `list` (PZMM saves them: it acks, and rewrites tags.txt). Shown at once; put back
--- if PZMM can't.
function PZMMMods:setTags(d, list)
    local before = self.tags[d.modId]
    self.tags[d.modId] = #list > 0 and list or nil
    local id = self:request("tags\t" .. d.modId .. "\t" .. table.concat(list, ";"), "Saving the tags of " .. d.name .. ".", true)
    if id then
        self.afterAck[id] = { function() end, "Tags", getTimestampMs(), nil, "Tags are edited in PZMM for now", function() self.tags[d.modId] = before end }
    end
    if self.filterTag then self:updateView() end
end

--- A tag as the app keeps them: trimmed, no ";" or tab, up to 40 characters.
local function cleanTag(t)
    return (tostring(t or ""):gsub("[;\t\r\n]", " "):gsub("^%s+", ""):gsub("%s+$", ""):sub(1, 40))
end

--- "B41 only" / "B42 only" when the mod's build folders don't match the running build, else false (read
--- once per mod: the folder the game picked for it, against its root).
function PZMMMods:buildTag(d)
    if d.buildTag == nil then
        d.buildTag = false
        pcall(function()
            local major = getCore():getGameVersion():getMajor()
            local function norm(p) return (tostring(p or ""):gsub("\\", "/"):gsub("/+$", "")) end
            local dir, vdir = norm(d.modInfo:getDir()), norm(d.modInfo:getVersionDir())
            local built = tonumber(vdir:sub(#dir + 1):match("^/(%d+)"))
            if not built then
                -- mod.info at the root: Build 41 style, unless it says it needs 42
                local vmin = d.modInfo:getVersionMin()
                if major >= 42 and not (vmin and vmin:getMajor() >= 42) then d.buildTag = "B41 only" end
            elseif built >= 42 and major < 42 then
                d.buildTag = "B42 only"
            elseif built < 42 and major >= 42 then
                -- only a 41/ folder: the game fell back to it
                d.buildTag = "B41 only"
            end
        end)
    end
    return d.buildTag
end

-- the right panel: the picked mod's details
function PZMMMods:drawSide()
    local x, y, w, h = self:sideBox()
    rect(self, x, y, w, h, C.panel, 1)
    rect(self, x, y, 1, h, C.line, 1)
    if self.selected or self.checkOpen then
        local what = self.checkOpen and "check" or self.selected
        if self.detailFor ~= what then self.detailFor, self.detailScroll = what, 0 end
        self:setStencilRect(x, y, w, h)
        local bottom
        if self.checkOpen then
            bottom = self:drawCheck(x + 18, y + 16 - self.detailScroll, w - 36, y + h)
        else
            bottom = self:drawDetails(self.selected, x + 18, y + 16 - self.detailScroll, w - 36, y + h)
        end
        self:clearStencilRect()
        local full = bottom + self.detailScroll - y + 16
        self.detailBox = { x = x, y = y, w = w, h = h, full = full }
        self.detailScroll = math.max(0, math.min(self.detailScroll, full - h))
        scrollbar(self, "detailScroll", x, y, w, h, full)
        return
    end
    self:drawOrder(x, y, w, h)
end

--- The load order (the app's side panel): enabled mods as the game loads them, a later one winning where
--- two change the same files. Its own scroll; a click picks the mod.
--- What a mod's shared-files mark says on hover.
function PZMMMods:overlapTip(ovl)
    local function names(set)
        local out = {}
        for id in pairs(set) do out[#out + 1] = (self.model.mods[id] and self.model.mods[id].name) or id end
        table.sort(out)
        return table.concat(out, ", ")
    end
    local t = "Shared files"
    local won, lost = names(ovl.wins), names(ovl.loses)
    if won ~= "" then t = t .. "\nOverrides files from " .. won end
    if lost ~= "" then t = t .. "\nSome files are overridden by " .. lost end
    return t
end

function PZMMMods:drawOrder(x, y, w, h)
    local mx, my = self:getMouseX(), self:getMouseY()
    local hx, hy = x + 18, y + 16
    local iw = w - 36
    bold(self, "Load order", hx, hy, C.text, MEDIUM)
    if mx >= hx and mx < x + w - 18 and my >= hy and my < hy + H_MEDIUM then
        tip(self, "Load order\nThe order the game loads your enabled mods: where two change the same files, the later one wins. Drag to reorder; click a mod for its details.")
    end
    local ly = hy + H_MEDIUM + 6
    for _, l in ipairs(wrap(T("order_lede", "Mods load top to bottom. When two change the same file, the lower one wins. The library is yours to organise: its order doesn't change this one."), iw)) do
        text(self, l, hx, ly, C.muted)
        ly = ly + H_SMALL + 2
    end
    ly = ly + 10
    -- three facts as raised tiles: Enabled, Problems (red when there are some), Overridden (once the scan has run through the mods)
    local nprob = self.oprobN or 0
    local ovr, ovDone = 0, false
    do
        local of, done = self:overlaps()
        ovDone = done
        for _, o in pairs(of) do
            for _ in pairs(o.loses) do ovr = ovr + 1; break end
        end
    end
    local facts = {
        { T("order_enabled", "Enabled"), tostring(#(self.order or {})) },
        { T("order_problems", "Problems"), tostring(nprob), nprob > 0 and C.danger or nil, "Problems\nEnabled mods that need something the load order or your mods don't give them." },
        { T("order_overridden", "Overridden"), ovDone and tostring(ovr) or "...", nil, "Overridden\nEnabled mods with files another mod replaces." },
    }
    local tw = math.floor((iw - 16) / 3)
    local th = 6 + H_SMALL + H_MEDIUM + 6
    for i, f in ipairs(facts) do
        local tx = hx + (i - 1) * (tw + 8)
        round(self, tx, ly, tw, th, 6, C.raised)
        text(self, fit(f[1], tw - 16), tx + 10, ly + 6, C.muted)
        bold(self, f[2], tx + 10, ly + 6 + H_SMALL, f[3] or C.text, MEDIUM)
        if f[4] and mx >= tx and mx < tx + tw and my >= ly and my < ly + th then tip(self, f[4]) end
    end
    ly = ly + th + 14
    -- the app's buttons, full width one under another: Autosort, Fix load order when there's something to fix, Sort A-Z
    if #(self.order or {}) > 0 then
        local function button(label, kind, tipText, fn, icon)
            local _, _, over = flatButton(self, label, hx, ly, mx, my, kind, icon, iw)
            if over then tip(self, tipText) end
            self.hits[#self.hits + 1] = { kind = "fn", fn = fn, x = hx, y = ly, w = iw, h = BAR }
            ly = ly + BAR + 8
        end
        button("Autosort", "primary", "Autosort\nTurns on what your mods need, puts frameworks first and translations last, then makes every requirement and load rule hold, keeping your order everywhere else.", function() self:autosortOrder() end)
        if (self.ofixable or 0) > 0 then
            button("Fix load order", nil, "Fix load order\nTurns on what's needed and moves as little as it can so every requirement and load rule holds.", function() self:fixOrder() end, TB.check)
        end
        if #self.order > 1 then button(T("order_sort_az", "Sort A-Z"), "quiet", "Sort A-Z\nBy name, with requirements and load rules still first.", function() self:sortOrderAz() end, TB.sort) end
        ly = ly + 4
    end
    ly = K.heading(self, T("order_mods", "Mods in load order"), hx, ly + 6)
    for _, l in ipairs(wrap(T("order_drag_hint", "Drag to reorder. Enter or double-click opens a mod."), iw)) do
        text(self, l, hx, ly, C.muted)
        ly = ly + H_SMALL + 2
    end
    ly = ly + 6
    local top = ly
    rect(self, x + 1, top - 1, w - 2, 1, C.line, 1)
    local rh = H_SMALL + 22
    local room = y + h - 10 - top
    local full = #(self.order or {}) * rh
    self.orderScroll = math.max(0, math.min(self.orderScroll or 0, full - room))
    self.orderBox = { x = x, y = top, w = w, h = room, full = full }
    if full == 0 then
        local hint = "Enable a mod and it shows here."
        text(self, hint, x + (w - width(hint)) / 2, top + 24, C.muted)
        return
    end
    -- a drag: started past 5 px; the list scrolls near its edges; it lands before the row nearest the mouse
    local od = self.odrag
    if od and not od.moved and math.abs(my - od.y0) >= 5 then od.moved = true end
    if od and od.moved then
        if my < top + 20 then self.orderScroll = math.max(0, self.orderScroll - 8) end
        if my > top + room - 20 then self.orderScroll = math.max(0, math.min(self.orderScroll + 8, full - room)) end
        od.to = math.max(1, math.min(#self.order + 1, math.floor((my - top + self.orderScroll) / rh + 0.5) + 1))
    end
    self:setStencilRect(x - 6, top, w + 6, room)
    local ry = top - self.orderScroll
    local nw = width(tostring(#self.order))
    for i, id in ipairs(self.order) do
        if ry + rh >= top and ry <= top + room then
            local d = self.model.mods[id]
            local over = not (od and od.moved) and mx >= x and mx < x + w and my >= math.max(ry, top) and my < math.min(ry + rh, top + room)
            if over or self.orderSel == id then
                -- as a mod row: the accent, a 1 px edge and the white marker; the clicked one a little stronger
                local ey, eh = math.max(ry, top), math.min(ry + rh, top + room) - math.max(ry, top)
                rect(self, x, ey, w, eh, C.accent, self.orderSel == id and 0.32 or 0.24)
                rect(self, x, ey, w, 1, C.accent, 0.7)
                rect(self, x, ey + eh - 1, w, 1, C.accent, 0.7)
                rect(self, x, ey, 1, eh, C.accent, 0.7)
                rect(self, x + w - 1, ey, 1, eh, C.accent, 0.7)
                if K.SEL_MARK then self:drawTextureScaled(K.SEL_MARK, x - 5, ry + math.floor(rh / 2) - 7, 10, 14, 1, 1, 1, 1) end
            end
            local mid = ry + (rh - H_SMALL) / 2
            text(self, tostring(i), hx + nw - width(tostring(i)), mid, over and C.onSelected or C.accentText)
            local name = d and d.name or id
            local prob = self.oprob and self.oprob[id]
            local c = (not d and C.danger) or (d.isAvailable == false and C.danger) or (over and C.onSelected) or C.text
            local ovm = d and d.isActive and self.ovOf and self.ovOf[id]
            local room2 = w - 36 - nw - 12 - (prob and 24 or 0) - (ovm and 24 or 0)
            local shown = fit(name, room2)
            bold(self, shown, hx + nw + 12, mid, c)
            -- its ID after the name, muted, when there's room
            local ix = hx + nw + 12 + width(shown) + 9
            if d and ix + width(id) < hx + nw + 12 + room2 then text(self, id, ix, mid, over and copy(C.onSelected, 0.72) or C.muted) end
            -- a problem: the app's alert mark at the right, its reason on hover
            local rx2 = x + w - 18 - 16
            local iconTip
            if ovm and TB.overlap then
                local wins = false
                for _ in pairs(ovm.wins) do wins = true; break end
                local oc = over and C.onSelected or (wins and C.accentText or C.muted)
                self:drawTextureScaled(TB.overlap, rx2, ry + math.floor((rh - 16) / 2), 16, 16, 1, oc.r, oc.g, oc.b)
                if over and mx >= rx2 - 4 and mx < rx2 + 20 then iconTip = self:overlapTip(ovm) end
                rx2 = rx2 - 24
            end
            if prob and TB.alert then
                self:drawTextureScaled(TB.alert, rx2, ry + math.floor((rh - 16) / 2), 16, 16, 1, C.danger.r, C.danger.g, C.danger.b)
            end
            if over then tip(self, iconTip or (name .. "\n" .. id .. (prob and ("\n" .. prob[1]) or ""))) end
            if od and od.moved and od.i == i then rect(self, x + 1, ry, w - 2, rh, C.panel, 0.6) end
            self.hits[#self.hits + 1] = { kind = "orow", i = i, id = id, mod = d, x = x, y = math.max(ry, top), w = w, h = math.min(ry + rh, top + room) - math.max(ry, top) }
        end
        ry = ry + rh
    end
    if od and od.moved and od.to and od.to ~= od.i and od.to ~= od.i + 1 then
        local ty = top - self.orderScroll + (od.to - 1) * rh
        if K.FEATHER then self:drawTextureScaled(K.FEATHER, x + 1, ty, w - 2, 20, 1, C.accent.r, C.accent.g, C.accent.b) end
        rect(self, x + 1, ty, w - 2, 1, C.dropEdge, 1)
    end
    self:clearStencilRect()
    if od and od.moved then
        local gw, gy = math.min(w, 320), my - math.floor(rh / 2)
        round(self, mx - 16, gy, gw, rh, 6, C.panel, 0.85)
        outline(self, mx - 16, gy, gw, rh, 6, C.accent, 0.7)
        local d = self.model.mods[od.id]
        bold(self, fit(d and d.name or od.id, gw - 24), mx - 4, gy + (rh - H_SMALL) / 2, C.text)
    end
    scrollbar(self, "orderScroll", x, top, w, room, full)
end

--- sorting_rules.txt: read it into FILE_RULES (and its raw lines, kept to write back), and forget every
--- mod's cached rules.
function PZMMMods:loadRuleFile()
    local lines = {}
    local ok = pcall(function()
        local r = getFileReader("sorting_rules.txt", false)
        if not r then return end
        local line = r:readLine()
        while line do
            -- the app writes CRLF: the "\r" is no part of the line
            if line:sub(-1) == "\r" then line = line:sub(1, -2) end
            lines[#lines + 1] = line
            line = r:readLine()
        end
        r:close()
    end)
    -- a read that failed part-way has only part of the file: writing it back would cut the rest off
    self.ruleBad = not ok
    -- CRLF, as the app writes it (the game's reader drops line ends, so a file's own can't be told apart)
    self.ruleCRLF = true
    self.ruleLines = lines
    for k in pairs(FILE_RULES) do FILE_RULES[k] = nil end
    local cur
    for _, l in ipairs(lines) do
        local t = l:gsub("^%s+", ""):gsub("%s+$", "")
        local head = t:match("^%[(.-)%]$")
        if head then
            cur = head:gsub("^%s+", ""):gsub("^\\+", ""):gsub("%s+$", "")
        else
            local k, v = t:match("^([%w_]+)%s*=%s*(.*)$")
            if k and cur then
                k = k:lower()
                local f = FILE_RULES[cur]
                if not f then
                    f = { after = {}, before = {}, incompatible = {}, first = "", last = "", category = "" }
                    FILE_RULES[cur] = f
                end
                local function ids(list)
                    for id in v:gmatch("[^,;]+") do
                        id = id:gsub("^%s+", ""):gsub("^\\+", ""):gsub("%s+$", "")
                        if id ~= "" then list[#list + 1] = id end
                    end
                end
                local function pos()
                    local p = v:lower():gsub("%s", "")
                    return (p == "on" or p == "true" or p == "0") and "on" or p == "category" and "category" or ""
                end
                if k == "loadafter" or k == "loadmodafter" then ids(f.after)
                elseif k == "loadbefore" or k == "loadmodbefore" then ids(f.before)
                elseif k == "incompatiblemods" or k == "incompatible" then ids(f.incompatible)
                elseif k == "loadfirst" then f.first = pos()
                elseif k == "loadlast" then f.last = pos()
                elseif k == "category" then f.category = (v:gsub("^%s+", ""):gsub("%s+$", "")) end
            end
        end
    end
    for _, d in pairs(self.model.mods) do d.rules = nil end
end

K.RULE_KEYS = {
    loadafter = { loadafter = true, loadmodafter = true }, loadbefore = { loadbefore = true, loadmodbefore = true },
    incompatiblemods = { incompatiblemods = true, incompatible = true },
    loadfirst = { loadfirst = true }, loadlast = { loadlast = true }, category = { category = true },
}
-- "*": every rule key (Clear rules)
K.ALL_RULE_KEYS = {}
for _, set in pairs(K.RULE_KEYS) do
    for k in pairs(set) do K.ALL_RULE_KEYS[k] = true end
end
K.RULE_KEYS["*"] = K.ALL_RULE_KEYS

--- Set one mod's rule line in sorting_rules.txt (`key`: loadAfter, loadBefore, incompatibleMods, loadFirst,
--- loadLast or category; `value` "" removes it; key "*" with "" removes them all), keeping every other line and
--- block of the file as it is. A mod with several [id] blocks: the first is edited, and the same keys go from
--- the others, so the file says what the screen shows.
function PZMMMods:setRule(id, key, value)
    self:loadRuleFile()
    if self.ruleBad then
        self:say("Zomboid/Lua/sorting_rules.txt couldn't be read to the end, so it was left as it is. Close what is using it and Rescan.")
        return false
    end
    local lines = self.ruleLines
    local alias = K.RULE_KEYS[key:lower()]
    local function head(l)
        local h = l:gsub("^%s+", ""):gsub("%s+$", ""):match("^%[(.-)%]$")
        if not h then return nil end
        return (h:gsub("^%s+", ""):gsub("^\\+", ""):gsub("%s+$", ""))
    end
    -- every block of this mod: its first line and its last
    local blocks, open = {}, nil
    for i, l in ipairs(lines) do
        local h = head(l)
        if h then
            if open then open.to = i - 1 end
            open = nil
            if h == id then
                open = { from = i }
                blocks[#blocks + 1] = open
            end
        end
    end
    if open then open.to = #lines end
    local from, to = blocks[1] and blocks[1].from, blocks[1] and blocks[1].to
    if not from then
        if value == "" then return true end
        lines[#lines + 1] = "[" .. id .. "]"
        lines[#lines + 1] = key .. "=" .. value
    else
        -- the later blocks lose these keys (and go, once nothing is left in them), the last first
        for b = #blocks, 2, -1 do
            local blk, removed = blocks[b], 0
            for i = blk.to, blk.from + 1, -1 do
                local k = lines[i]:match("^%s*([%w_]+)%s*=")
                if k and alias[k:lower()] then
                    table.remove(lines, i)
                    removed = removed + 1
                end
            end
            local empty = true
            for i = blk.from + 1, blk.to - removed do
                if lines[i] and lines[i]:match("%S") then empty = false end
            end
            if empty then
                for i = blk.to - removed, blk.from, -1 do table.remove(lines, i) end
            end
        end
        local hits = {}
        for i = from + 1, to do
            local k = lines[i]:match("^%s*([%w_]+)%s*=")
            if k and alias[k:lower()] then hits[#hits + 1] = i end
        end
        if value ~= "" then
            if #hits > 0 then lines[hits[1]] = key .. "=" .. value else table.insert(lines, to + 1, key .. "=" .. value) end
            for j = #hits, 2, -1 do table.remove(lines, hits[j]) end
        else
            for j = #hits, 1, -1 do table.remove(lines, hits[j]) end
            -- a block left with nothing in it goes
            local last = to - #hits
            local empty = true
            for i = from + 1, last do
                if lines[i] and lines[i]:match("%S") then empty = false end
            end
            if empty then
                for i = last, from, -1 do table.remove(lines, i) end
            end
        end
    end
    local eol = self.ruleCRLF and "\r\n" or "\n"
    local ok = pcall(function()
        local w = getFileWriter("sorting_rules.txt", true, false)
        w:write(table.concat(lines, eol) .. (#lines > 0 and eol or ""))
        w:close()
    end)
    self:loadRuleFile()
    self:updateView()
    if not ok then self:say("Couldn't save Zomboid/Lua/sorting_rules.txt.") end
    return ok
end

--- The Load rules section of a mod's details: loadFirst / loadLast / category as select-like fields and the
--- loadAfter / loadBefore / incompatible lists (add by prompt, remove with the x), from mod.info and Mod Load
--- Order Sorter's rules file; Clear rules removes this mod's rules from the file.
function PZMMMods:drawRules(d, x, y, w)
    local mx, my = self:getMouseX(), self:getMouseY()
    local f = FILE_RULES[d.modId] or { after = {}, before = {}, incompatible = {}, first = "", last = "", category = "" }
    local r = rules(d)
    y = K.heading(self, T("rules_title", "Load rules"), x, y)
    local rh = H_SMALL + 6
    local fh = H_SMALL + 10
    local POS = { [""] = "No", on = "Of the whole list", category = "Of its category" }
    -- a field, as a select: raised, a line outline (accent on hover) and a chevron
    local function field(label, value, muted, tipText, fn, chev)
        text(self, label, x, y + (fh - H_SMALL) / 2, C.muted)
        local fx, fw = x + 130, w - 130
        local over = mx >= fx and mx < fx + fw and my >= y and my < y + fh
        round(self, fx, y, fw, fh, 6, C.raised)
        outline(self, fx, y, fw, fh, 6, over and C.accent or C.line, over and 0.7 or 1)
        text(self, fit(value, fw - 20 - (chev and 22 or 0)), fx + 10, y + (fh - H_SMALL) / 2, muted and C.muted or C.text)
        if chev and K.CHEV_DOWN then self:drawTextureScaled(K.CHEV_DOWN, fx + fw - 22, y + math.floor((fh - 12) / 2), 12, 12, 1, C.muted.r, C.muted.g, C.muted.b) end
        if over then tip(self, label .. "\n" .. tipText) end
        self.hits[#self.hits + 1] = { kind = "fn", x = fx, y = y, w = fw, h = fh, fn = fn }
        y = y + fh + 4
    end
    local function setting(label, key, cur, tipText)
        field(label, POS[cur] or "No", cur == "", tipText, function()
            local items = {}
            for _, o in ipairs({ { "", "No" }, { "on", "Of the whole list" }, { "category", "Of its category" } }) do
                items[#items + 1] = { o[2], function() self:setRule(d.modId, key, o[1]) end, cur == o[1] }
            end
            self:menu(items)
        end, true)
    end
    setting("Load first", "loadFirst", f.first, "Autosort puts it before the rest: in the whole list, or first within its category.")
    setting("Load last", "loadLast", f.last, "Autosort puts it after the rest: in the whole list, or last within its category.")
    field("Sorter category", f.category ~= "" and f.category or "None", f.category == "", "Mod Load Order Sorter's category for it (tweaks, resource, map...). Click to type one; empty removes it.", function()
        self:prompt("Category (empty for none)", f.category, "Save", function(typed)
            typed = (tostring(typed or ""):gsub("[\r\n]", " "):gsub("^%s+", ""):gsub("%s+$", ""))
            self:setRule(d.modId, "category", typed)
        end)
    end)
    -- `ask`: the prompt's title; `same`: what a mod can't be told about itself
    local function list(label, key, info, mine, word, ask, same)
        text(self, label, x, y, C.muted)
        local first = true
        local function row(id, own)
            local m = self.model.mods[id]
            local name = m and m.name or id
            local ry = y
            text(self, fit(name, w - 130 - (own and 22 or width("mod.info") + 8)), x + 130, ry, m and C.text or C.muted)
            if own then
                local ox = mx >= x + w - 20 and mx < x + w and my >= ry and my < ry + rh
                local xc = ox and C.danger or C.muted
                if TB.x then self:drawTextureScaled(TB.x, x + w - 16, ry + 1, 12, 12, 1, xc.r, xc.g, xc.b) end
                if ox then tip(self, "Remove\nTakes " .. id .. " out of this mod's " .. key .. " in sorting_rules.txt.") end
                self.hits[#self.hits + 1] = { kind = "fn", x = x + w - 20, y = ry, w = 20, h = rh, fn = function()
                    local rest = {}
                    for _, o in ipairs(mine) do if o ~= id then rest[#rest + 1] = o end end
                    self:setRule(d.modId, key, table.concat(rest, ","))
                end }
            else
                text(self, "mod.info", x + w - width("mod.info"), ry, C.muted)
            end
            y = y + rh
            first = false
        end
        for _, id in ipairs(info) do row(id, false) end
        for _, id in ipairs(mine) do
            local dup = false
            for _, o in ipairs(info) do if o == id then dup = true end end
            if not dup then row(id, true) end
        end
        if first then
            text(self, "None", x + 130, y, C.muted)
            y = y + rh
        end
        local aw = width("+ Add") + 8
        local over = mx >= x + 130 and mx < x + 130 + aw and my >= y and my < y + rh
        text(self, "+ Add", x + 130, y, over and C.accentText or C.muted)
        if over then tip(self, "Add\nA mod ID (or a mod's name) " .. word .. ". Saved in Zomboid/Lua/sorting_rules.txt.") end
        self.hits[#self.hits + 1] = { kind = "fn", x = x + 130, y = y, w = aw, h = rh, fn = function()
            self:prompt(ask, "", "Add", function(typed)
                typed = (tostring(typed or ""):gsub("^%s+", ""):gsub("%s+$", ""))
                if typed == "" then return end
                local id = typed
                for mid, m in pairs(self.model.mods) do
                    if mid:lower() == typed:lower() or (m.name or ""):lower() == typed:lower() then id = mid; break end
                end
                if id == d.modId then return self:say(same) end
                local all = {}
                for _, o in ipairs(mine) do
                    if o == id then return end
                    all[#all + 1] = o
                end
                all[#all + 1] = id
                self:setRule(d.modId, key, table.concat(all, ","))
            end)
        end }
        y = y + rh
    end
    list("Load after", "loadAfter", r.infoAfter, f.after, "this mod should load after", "Load after which mod? (its ID or name)", "A mod can't load after itself.")
    list("Load before", "loadBefore", r.infoBefore, f.before, "this mod should load before", "Load before which mod? (its ID or name)", "A mod can't load before itself.")
    local bad = {}
    for id in pairs(d.incompatibleWith or {}) do bad[#bad + 1] = id end
    table.sort(bad)
    list("Incompatible with", "incompatibleMods", bad, f.incompatible, "that doesn't work with this one", "Incompatible with which mod? (its ID or name)", "A mod can't be incompatible with itself.")
    -- anything set in the file: one button takes it all out
    if #f.after > 0 or #f.before > 0 or #f.incompatible > 0 or f.first ~= "" or f.last ~= "" or f.category ~= "" then
        y = y + 4
        local label = T("rules_clear", "Clear rules")
        local bw, _, over = flatButton(self, label, x, y, mx, my, "quiet")
        if over then tip(self, "Clear rules\nRemoves this mod's rules from Zomboid/Lua/sorting_rules.txt. What its mod.info says stays.") end
        self.hits[#self.hits + 1] = { kind = "fn", x = x, y = y, w = bw, h = BAR, fn = function() self:setRule(d.modId, "*", "") end }
        y = y + BAR
    end
    return y
end

function PZMMMods:drawDetails(d, x, y, w, bottom)
    local info = d.modInfo
    if d.poster == nil then
        d.poster = false
        pcall(function() if (info:getPosterCount() or 0) > 0 then d.poster = getTexture(info:getPoster(0)) or false end end)
        -- none of its own: the Workshop picture PZMM saved for the game, if the game can load it from there
        if not d.poster and self.app then
            pcall(function()
                local ws = wsOf(d)
                if ws ~= "" then
                    local sep = getFileSeparator()
                    d.poster = getTexture(Core.getMyDocumentFolder() .. sep .. "Lua" .. sep .. "pzmm" .. sep .. "img" .. sep .. ws .. ".png") or false
                end
            end)
        end
    end
    local top0 = y
    if d.poster then
        -- at most 220 high, cropped to fill the width (cover), cut to its own stencil as the hero's art is
        local pw0, phh = math.max(1, d.poster:getWidth()), math.max(1, d.poster:getHeight())
        local ph = math.min(220, math.floor(w * phh / pw0))
        rect(self, x, y, w, ph, C.raised, 1)
        local bx0, by0, bw0, bh0 = self:sideBox()
        local c0, c1 = math.max(y, by0), math.min(y + ph, by0 + bh0)
        if c1 > c0 then
            self:clearStencilRect()
            self:setStencilRect(x, c0, w, c1 - c0)
            local sc = math.max(w / pw0, ph / phh)
            local aw, ah = pw0 * sc, phh * sc
            self:drawTextureScaled(d.poster, x + (w - aw) / 2, y + (ph - ah) / 2, aw, ah, 1, 1, 1, 1)
            self:clearStencilRect()
            self:setStencilRect(bx0, by0, bw0, bh0)
        end
        y = y + ph + 14
    end
    local mx, my = self:getMouseX(), self:getMouseY()
    local nameW = d.poster and w or (w - 48)
    for _, l in ipairs(wrap(d.name, nameW, MEDIUM)) do
        bold(self, l, x, y, C.text, MEDIUM)
        y = y + H_MEDIUM + 2
    end
    -- its ID under the title
    text(self, fit(d.modId, w), x, y, C.muted)
    y = y + H_SMALL + 10
    -- the close X at the top right, as the app's details (over the picture, if it has one)
    local cx0, cy0 = x + w - 32 - 8, top0 + 6
    local cover = mx >= cx0 and mx < cx0 + 32 and my >= cy0 and my < cy0 + 32
    round(self, cx0 - 1, cy0 + 1, 34, 36, 8, C.black, 0.28 * SHADOW.k)
    round(self, cx0 - 2, cy0 + 2, 36, 40, 8, C.black, 0.14 * SHADOW.k)
    round(self, cx0, cy0, 32, 32, 8, cover and C.hover or C.panel)
    outline(self, cx0, cy0, 32, 32, 8, cover and C.accent or C.line, cover and 0.7 or 1)
    if TB.x then
        local xc = cover and C.onSelected or C.text
        self:drawTextureScaled(TB.x, cx0 + 8, cy0 + 8, 16, 16, 1, xc.r, xc.g, xc.b)
    end
    if cover then tip(self, "Close\nBack to the load order.") end
    self.hits[#self.hits + 1] = { kind = "fn", fn = function() self.selected = nil end, x = cx0, y = cy0, w = 32, h = 32 }
    if d.ghost then
        for _, l in ipairs(wrap(T("ghost_warn", "This mod is enabled in your profile but isn't installed. Install it, subscribe to it in Steam, or disable it."), w)) do
            text(self, l, x, y, C.text)
            y = y + H_SMALL + 2
        end
        y = y + 10
        local ax = x
        for _, a in ipairs({
            { "Disable", nil, function() self:toggle(d) end, TB.x },
            { "Find on the Workshop", nil, function() self:findOnWorkshop(d.modId) end, TB.steam },
        }) do
            local aw = width(a[1]) + 28 + 20
            if ax > x and ax + aw > x + w then ax, y = x, y + BAR + 6 end
            flatButton(self, a[1], ax, y, mx, my, a[2], a[4])
            self.hits[#self.hits + 1] = { kind = "fn", fn = a[3], x = ax, y = y, w = aw, h = BAR }
            ax = ax + aw + 6
        end
        return y + BAR + 14
    end
    -- its actions: on or off (red when off), its files, its Workshop page (favourite is in the row's menu)
    local ws = wsOf(d)
    local acts = { { d.isActive and "Disable" or "Enable", not d.isActive and "primary" or nil, function() self:toggle(d) end, d.isActive and TB.x or TB.check } }
    acts[#acts + 1] = { "Show files", nil, function() pcall(function() showFolderInDesktop(d.modInfo:getDir()) end) end, TB.folder }
    if ws and ws ~= "" and ws ~= "0" then acts[#acts + 1] = { "Workshop page", nil, function() self:openWorkshop(ws) end, TB.steam } end
    local ax = x
    for _, a in ipairs(acts) do
        local aw = width(a[1]) + 28 + (a[4] and 20 or 0)
        if ax > x and ax + aw > x + w then ax, y = x, y + BAR + 6 end
        flatButton(self, a[1], ax, y, mx, my, a[2], a[4])
        self.hits[#self.hits + 1] = { kind = "fn", fn = a[3], x = ax, y = y, w = aw, h = BAR }
        ax = ax + aw + 6
    end
    y = y + BAR + 14
    local function field(label, value)
        if not value or value == "" then return end
        text(self, label, x, y, C.muted)
        text(self, fit(tostring(value), w - 110), x + 110, y, C.text)
        y = y + H_SMALL + 6
    end
    field("Source", pzmmCopy(d) and T("details_source_pzmm", "Workshop: PZMM's copy of a Workshop item") or (ws and ws ~= "" and ws ~= "0") and T("details_source_ws", "Subscribed on the Workshop") or T("details_source_local", "Local file"))
    -- Steam says a newer version is waiting: the app's update box
    local stNew = self:steamOf(d)
    if stNew and stNew.state == "NeedsUpdate" then
        local label = T("details_update_now", "Update now")
        local bw = width(label) + 48
        local lines = wrap(stNew.pzmm and K.PZMM_UPDATE[3]:gsub("^[^\n]*\n", "") or T("details_newer", "A newer version is on the Workshop."), w - bw - 36)
        local lh = #lines * (H_SMALL + 2)
        local bh = math.max(BAR, lh) + 16
        round(self, x, y, w, bh, 6, C.accent, 0.32)
        for i, l in ipairs(lines) do text(self, l, x + 12, y + math.floor((bh - lh) / 2) + (i - 1) * (H_SMALL + 2), C.text) end
        local bx, by = x + w - 12 - bw, y + math.floor((bh - BAR) / 2)
        local _, _, uo = flatButton(self, label, bx, by, mx, my, "primary", TB.update)
        if uo then tip(self, self.app and "Update now\nOpens PZMM, which updates it once you say yes." or "Update now\nSteam downloads it when it next can: restarting Steam gets it now. Opens its Workshop page.") end
        self.hits[#self.hits + 1] = { kind = "fn", x = bx, y = by, w = bw, h = BAR, fn = function()
            if self.app and ws then self:request("install\t" .. ws, d.name .. " update asked for.") elseif ws then self:openWorkshop(ws) end
        end }
        y = y + bh + 8
    end
    pcall(function() field("Author", info:getAuthor()) end)
    pcall(function() field(T("details_mod_version", "Mod version"), info:getModVersion()) end)
    -- PZMM's category (from Workshop tags) when it has one, else the mod's own
    local cats = self.cats or readCategories() or { of = {} }
    self.cats = cats
    local own = { map = "Map", vehicle = "Vehicles", features = "Features", modpack = "Mod pack" }
    field("Category", cats.of[d.modId] or own[d.category or ""])
    local tg = (self.tags and self.tags[d.modId]) or {}
    if #tg > 0 or self.app then
        text(self, T("details_tags", "Tags"), x, y, C.muted)
        local tx0, tx, ty0 = x + 110, x + 110, y - 1
        for _, t in ipairs(tg) do
            local cw = width(t) + 17 + (self.app and 18 or 0)
            if tx + cw > x + w and tx > tx0 then tx, ty0 = tx0, ty0 + H_SMALL + 8 end
            local right = tagChip(self, t, tx, ty0)
            if self.app then
                local ox = right + 2
                local over = mx >= ox and mx < ox + 14 and my >= ty0 and my < ty0 + H_SMALL + 2
                if TB.x then self:drawTextureScaled(TB.x, ox, ty0 + 2, 12, 12, 1, (over and C.danger or C.muted).r, (over and C.danger or C.muted).g, (over and C.danger or C.muted).b) end
                if over then tip(self, "Remove tag\n" .. t) end
                local gone = t
                self.hits[#self.hits + 1] = { kind = "fn", fn = function()
                    local list = {}
                    for _, o in ipairs(self.tags[d.modId] or {}) do if o ~= gone then list[#list + 1] = o end end
                    self:setTags(d, list)
                end, x = ox, y = ty0, w = 14, h = H_SMALL + 2 }
                right = ox + 14
            end
            tx = right + 6
        end
        if self.app then
            local label = T("details_add_tag", "Add tag")
            local bw = width(label) + 28
            if tx + bw > x + w and tx > tx0 then tx, ty0 = tx0, ty0 + H_SMALL + 8 end
            local _, _, over = flatButton(self, label, tx, ty0 - 7, mx, my, "quiet")
            if over then tip(self, "Add tag\nYour own word for this mod: filter the list by it in Filters. PZMM keeps it.") end
            self.hits[#self.hits + 1] = { kind = "fn", fn = function()
                self:prompt("Add a tag to " .. d.name, "", "Add", function(t)
                    t = cleanTag(t)
                    if t == "" then return end
                    local list = {}
                    for _, o in ipairs(self.tags[d.modId] or {}) do
                        if o:lower() == t:lower() then return end
                        list[#list + 1] = o
                    end
                    if #list >= 50 then return self:say("A mod can have 50 tags.") end
                    list[#list + 1] = t
                    self:setTags(d, list)
                end)
            end, x = tx, y = ty0 - 7, w = bw, h = BAR }
            ty0 = ty0 + BAR - 7 - (H_SMALL + 2) + 4
        end
        y = ty0 + H_SMALL + 8
    end
    pcall(function() field("Workshop", wsOf(d)) end)
    local stu = self:steamOf(d)
    if stu and (stu.updated or 0) > 0 then
        local days = math.floor((now() - stu.updated) / 86400)
        field("Updated", days <= 0 and "today" or days == 1 and "yesterday" or days < 60 and (days .. " days ago") or (math.floor(days / 30) .. " months ago"))
    end
    if stu and (stu.size or 0) > 0 then field("Size", (math.floor(stu.size / 104857.6) / 10) .. " MB") end
    -- when it first arrived on this PC, as the app knows it (firstseen.txt; none without the app)
    local seen = self.firstSeen and self.firstSeen[d.modId]
    if seen then
        local fy = y
        local ok, when = pcall(os.date, "%d %B %Y", K.LT(seen))
        field(T("details_first_seen", "First seen"), ok and when or "")
        local hx, hy = self:getMouseX(), self:getMouseY()
        if hx >= x and hx < x + w and hy >= fy and hy < y then
            tip(self, T("details_first_seen", "First seen") .. "\n" .. T("details_first_seen_tip", "When this mod arrived on this PC, as far as PZMM can tell. It can reset if Steam downloads the mod again."))
        end
    end
    field(T("details_langs", "Has texts in"), self:langsOf(d))
    pcall(function()
        local lo, hi = info:getVersionMin(), info:getVersionMax()
        local v = ""
        if lo and hi then v = lo:toString() .. " to " .. hi:toString()
        elseif lo then v = T("details_version_or_newer", "%1 or newer", lo:toString())
        elseif hi then v = "up to " .. hi:toString() end
        field(T("details_needs_game", "Needs game version"), v)
    end)
    local ids = {}
    for id, _ in pairs(d.requireMods or {}) do ids[#ids + 1] = id end
    table.sort(ids)
    if #ids > 0 then
        -- each required mod and where it stands: on, off, or not installed
        y = K.heading(self, T("details_requires", "Requires"), x, y + 6)
        for _, id in ipairs(ids) do
            local r = self.model.mods[id]
            local state, c = "not installed", C.danger
            if r and r.isActive then state, c = "on", C.muted elseif r then state = "off" end
            local label = r and r.name or id
            local over = r and mx >= x and mx < x + w and my >= y and my < y + H_SMALL + 6
            text(self, fit(label, w - width(state) - 12), x, y, over and C.accentText or C.text)
            text(self, state, x + w - width(state), y, c)
            if r then
                if over then tip(self, label .. "\n" .. id .. "\nClick to see it.") end
                self.hits[#self.hits + 1] = { kind = "fn", fn = function() self.selected = r end, x = x, y = y, w = w, h = H_SMALL + 6 }
            end
            y = y + H_SMALL + 6
        end
    end
    -- Workshop items it needs that you aren't subscribed to: Subscribe opens them in Steam
    local st = self:steamOf(d)
    for _, item in ipairs(st and st.missing or {}) do
        local title = (self.steam[item] and self.steam[item].title) or ("Workshop item " .. item)
        text(self, fit(title, w - 130), x, y + (BAR - H_SMALL) / 2, C.text)
        local label = self.app and "Install with PZMM" or "Subscribe"
        local bw = width(label) + 48
        local bx = x + w - bw
        local _, _, over = flatButton(self, label, bx, y, self:getMouseX(), self:getMouseY(), "primary", TB.download)
        if over then tip(self, self.app and (label .. "\nOpens PZMM, which installs " .. title .. " once you say yes.") or ("Subscribe\nOpens " .. title .. " in Steam: subscribe there and it downloads.")) end
        local act = self.app and function() self:request("install\t" .. item, title .. " asked for.") end or function() self:openWorkshop(item) end
        self.hits[#self.hits + 1] = { kind = "fn", fn = act, x = bx, y = y, w = bw, h = BAR }
        y = y + BAR + 6
    end
    -- the enabled mods that need it, and the mods it can't run with (red when on): click one to see it
    local function mods(label, list, red, head)
        if #list == 0 then return end
        table.sort(list, byName)
        local lx = x + 110
        if head then
            y = K.heading(self, label, x, y + 6)
            lx = x
        else
            text(self, label, x, y, C.muted)
        end
        for _, m in ipairs(list) do
            local over = mx >= lx and mx < x + w and my >= y and my < y + H_SMALL + 6
            text(self, fit(m.name, w - (lx - x)), lx, y, over and C.accentText or (red and m.isActive and C.danger) or C.text)
            if over then tip(self, m.name .. "\n" .. m.modId .. (red and m.isActive and "\nOn now: they can't both be on." or "") .. "\nClick to see it.") end
            self.hits[#self.hits + 1] = { kind = "fn", fn = function() self.selected = m end, x = lx, y = y, w = w - (lx - x), h = H_SMALL + 6 }
            y = y + H_SMALL + 6
        end
    end
    local users, clash = {}, {}
    for _, m in pairs(self.model.mods) do
        if m.isActive and m.requireMods and m.requireMods[d.modId] then users[#users + 1] = m end
    end
    for id, _ in pairs(d.incompatibleWith or {}) do
        if self.model.mods[id] then clash[#clash + 1] = self.model.mods[id] end
    end
    mods(T("details_needed_by", "Needed by"), users, nil, true)
    mods("Incompatible", clash, true)
    -- same-named Lua and script files: it replaces mods before it, and mods after it replace it
    if d.isActive then
        local of, done = self:overlaps()
        local o = of[d.modId]
        local shared = false
        local function counted(label, set)
            local list = {}
            for id, n in pairs(set or {}) do
                local m = self.model.mods[id]
                if m then list[#list + 1] = { m = m, n = n } end
            end
            if #list == 0 then return end
            table.sort(list, function(a, b) return byName(a.m, b.m) end)
            if not shared then
                shared = true
                y = K.heading(self, T("details_shared", "Shared files"), x, y + 6)
            end
            text(self, label, x, y, C.muted)
            for _, e in ipairs(list) do
                local over = mx >= x + 110 and mx < x + w and my >= y and my < y + H_SMALL + 6
                local files = e.n .. (e.n == 1 and " file" or " files")
                text(self, fit(e.m.name, w - 110 - width(files) - 12), x + 110, y, over and C.accentText or C.text)
                text(self, files, x + w - width(files), y, C.muted)
                if over then tip(self, e.m.name .. "\nSame-named files in media/lua or media/scripts: the mod later in the load order wins.\nClick to see it.") end
                local m = e.m
                self.hits[#self.hits + 1] = { kind = "fn", fn = function() self.selected = m end, x = x + 110, y = y, w = w - 110, h = H_SMALL + 6 }
                y = y + H_SMALL + 6
            end
        end
        if o then
            counted("Replaces", o.wins)
            counted("Replaced by", o.loses)
        elseif not done then
            text(self, "Looking for files it shares with other mods...", x, y, C.muted)
            y = y + H_SMALL + 6
        end
    end
    y = self:drawRules(d, x, y + 6, w) + 8
    if not (d.descLines and d.descW == w) then
        local desc = ""
        pcall(function() desc = info:getDescription() or "" end)
        -- the game's <LINE> and <BR> are line breaks; other tags and BBCode go
        desc = desc:gsub("<[Ll][Ii][Nn][Ee]>", "\n"):gsub("<[Bb][Rr]%s*/?>", "\n"):gsub("<[^>]*>", " "):gsub("%[[^%]]*%]", "")
        d.descLines, d.descW = wrap(desc, w), w
    end
    if #d.descLines > 1 or (d.descLines[1] or "") ~= "" then y = K.heading(self, T("details_description", "Description"), x, y + 6) end
    for _, l in ipairs(d.descLines) do
        if y + H_SMALL >= 0 and y <= bottom then text(self, l, x, y, C.desc) end
        y = y + H_SMALL + 2
    end
    return y
end

-- --- What's new: the app's feed (Zomboid/Lua/pzmm/feed.txt, written when it loads change notes),
-- drawn as the app's What's new tab: scraps of paper for the latest, then every change note by day

K.INK, K.INK_SOFT, K.BLOOD = hex("24211c"), hex("5d574c"), hex("6a0a0d")
K.SCRAPS = {}
for i = 1, 8 do K.SCRAPS[i] = tex("scrap_" .. i) end
local function inside(b, x, y) return b and x >= b.x and x < b.x + b.w and y >= b.y and y < b.y + b.h end
K.ROWS = 2 -- rows of scraps on the board; the rest are in the history
K.SHOWN_NOTES = 3 -- notes an open entry shows before "Show older"
K.FINE = "\"May break saves\" is a hint: PZMM flags map mods, new major versions and notes that mention a new game or saves. Back up a save before you load it with these."

--- The game opens only its own sites (LuaManager.isIndieStoneUrl: steamcommunity.com, projectzomboid.com,
--- theindiestone.com, pzwiki.net) and drops the rest without a word. Any other link goes through Steam's
--- link filter, which is on that list and passes you on to the site.
K.OWN_SITES = { "https://steamcommunity.com", "https://projectzomboid.com", "https://theindiestone.com", "https://pzwiki.net" }
local function allowed(url)
    for _, site in ipairs(K.OWN_SITES) do
        if url == site or url:sub(1, #site + 1) == site .. "/" then return true end
    end
    return false
end
--- Percent-encoded as UTF-8: A-Z a-z 0-9 - _ . ~ stay; a Java char over 127 is its code point, so it becomes
--- 2 or 3 bytes, and a surrogate pair (one character above U+FFFF) 4.
local function urlEncode(str)
    str = tostring(str)
    local out, i, n = {}, 1, #str
    local function bytes(...)
        for _, b in ipairs({ ... }) do out[#out + 1] = string.format("%%%02X", b) end
    end
    while i <= n do
        local b = str:byte(i)
        i = i + 1
        if (b >= 65 and b <= 90) or (b >= 97 and b <= 122) or (b >= 48 and b <= 57) or b == 45 or b == 95 or b == 46 or b == 126 then
            out[#out + 1] = str:sub(i - 1, i - 1)
        elseif b < 128 then
            bytes(b)
        elseif b < 0x800 then
            bytes(0xC0 + math.floor(b / 64), 0x80 + b % 64)
        elseif b >= 0xD800 and b <= 0xDBFF and i <= n and str:byte(i) >= 0xDC00 and str:byte(i) <= 0xDFFF then
            local cp = 0x10000 + (b - 0xD800) * 1024 + (str:byte(i) - 0xDC00)
            i = i + 1
            bytes(0xF0 + math.floor(cp / 262144), 0x80 + math.floor(cp / 4096) % 64, 0x80 + math.floor(cp / 64) % 64, 0x80 + cp % 64)
        else
            bytes(0xE0 + math.floor(b / 4096), 0x80 + math.floor(b / 64) % 64, 0x80 + b % 64)
        end
    end
    return table.concat(out)
end

--- The browser, or the Steam overlay where the game can't open one.
local function openLink(url)
    if not allowed(url) then
        url = "https://steamcommunity.com/linkfilter/?u=" .. urlEncode(url)
    end
    if not pcall(function() openUrl(url) end) then pcall(function() activateSteamOverlayToWebPage(url) end) end
end

--- A Workshop item's page: the Steam overlay, else the browser.
--- Search the Workshop for a mod ID (a mod that's enabled but not installed).
function PZMMMods:findOnWorkshop(id)
    openLink("https://steamcommunity.com/workshop/browse/?appid=108600&searchtext=" .. urlEncode(id))
end

function PZMMMods:openWorkshop(id)
    local overlay = false
    pcall(function() overlay = isSteamOverlayEnabled() end)
    if overlay then
        pcall(function() activateSteamOverlayToWorkshopItem(id) end)
    else
        openLink("https://steamcommunity.com/sharedfiles/filedetails/?id=" .. id)
    end
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
    -- BBCode and Markdown lists become "- " lines (before the marks are stripped)
    str = str:gsub("%[%*%]%s*", "\n- "):gsub("%[/?[Oo]?[Ll][Ii][Ss][Tt]%]", "\n"):gsub("\n[ \t]*[%-%*\226][ \t]+", "\n- "):gsub("^[%-%*][ \t]+", "- ")
    str = str:gsub("%[([^%]]*)%]%(([^)]*)%)", "%1"):gsub("%[/?[%a%*][^%]]*%]", ""):gsub("\n#+%s*", "\n"):gsub("^#+%s*", ""):gsub("[%*`]+", ""):gsub("%f[%w_]_+([^_%c]-)_+%f[^%w_]", "%1"):gsub("\n\n\n+", "\n\n")
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
    local ok, out = pcall(function() return os.date(long and "%A %d %B %Y" or "%d %B %Y", K.LT(t)) end)
    return ok and out or ""
end
K.RISKS = { map = "It adds a map.", version = "A major version jump.", notes = "Its notes mention a new game or wiped saves." }
local function riskTip(e)
    local why = {}
    for _, r in ipairs(e.risks) do why[#why + 1] = K.RISKS[r] or r end
    return "May break saves\n" .. table.concat(why, " ") .. "\nBack up a save before you load it with this."
end
local function scrapOf(item)
    local n = 0
    for i = 1, #item do n = n + item:byte(i) end
    return n % 8 + 1
end

--- Opening What's new clears its badge, until a newer update comes (called while it's drawn: writes only on news).
function PZMMMods:sawFeed()
    local f = self.feed or self:liteFeed()
    local newest = self.feedSeen or 0
    for _, e in ipairs(f and f.entries or {}) do newest = math.max(newest, e.updated) end
    if newest == self.feedSeen then return end
    self.feedSeen = newest
    local mx = math.max(newest, readFeedSeen())
    pcall(function()
        local w = getFileWriter("pzmm/feedseen.txt", true, false)
        w:write(tostring(mx))
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
    round(ui, x, y, w, h, 6, over and K.BLOOD or hex("ffffff"), over and 1 or 0.25)
    outline(ui, x, y, w, h, 6, over and K.BLOOD or K.INK, over and 1 or 0.35)
    text(ui, label, x + 10, y + 5, over and hex("ffffff") or K.INK)
    return w, h
end

K.UH_OH = tex("uh_oh")
K.SORRY = "Because of limits on what mods can do inside the game, What's new only works with the PZMM Mod Manager app installed. The app fetches your mods' change notes from Steam and brings them here."
K.SORRY_LINKS = {
    { icon = LINKS[1][1], url = K.RELEASES_URL, title = "Download on GitHub", sub = "Get the PZMM Mod Manager app" },
    { icon = LINKS[2][1], url = LINKS[2][2], title = "Join the Discord", sub = "Help, ideas and the community" },
}

--- What can't work here: the raccoon's sign, why (`body`, What's new's by default), and its `links` (the
--- app's GitHub and Discord by default), filling `w` by `h`. Returns its height.
function PZMMMods:drawSorry(x, y, w, h, mx, my, body, links, title)
    body, links, title = body or K.SORRY, links or K.SORRY_LINKS, title or "We're sorry"
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
    round(self, x, y, w, h, 8, C.panel)
    outline(self, x, y, w, h, 8, C.line, 1)
    image(self, K.UH_OH, gx, y + math.floor((h - img) / 2), img, img)
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
    local t = K.SCRAPS[scrapOf(e.item)]
    if t then self:drawTextureScaled(t, x - 16 * k, y - 16 * k, 332 * k, 262 * k, 1, tint, tint, tint) end
    local px, pw = x + 20, w - 40
    local ty = y + 22
    local title = e.title ~= "" and e.title or e.item
    for i, l in ipairs(wrap(title, pw, MARKER)) do
        if i > 2 then break end
        text(self, l, px, ty, K.INK, MARKER)
        ty = ty + H_MARKER
    end
    -- the title shows the mod in My mods
    self.feedHits[#self.feedHits + 1] = { kind = "show", e = e, x = px, y = y + 22, w = pw, h = ty - y - 22 }
    if mx >= px and mx < px + pw and my >= y + 22 and my < ty then tip(self, "Show it in My mods") end
    local meta = date(e.updated)
    if e.from ~= "" then meta = meta .. "    " .. e.from .. " -> " .. e.to end
    if not on then meta = meta .. "    Disabled" end
    text(self, meta, px, ty + 2, K.INK_SOFT)
    ty = ty + H_SMALL + 10
    local actionsY = y + h - 16 - (H_SMALL + 10)
    local body = excerpt(e)
    local italic = body == ""
    if italic then body = ((self.feed == nil or not e.loaded) and "Its change notes are on the Workshop: Read all notes opens them.") or ((#e.notes == 0 and #e.logs == 0) and "The author didn't write change notes for this update." or "(no text)") end
    for _, l in ipairs(wrap(body, pw)) do
        if ty + H_SMALL > actionsY - 6 then break end
        text(self, l, px, ty, italic and K.INK_SOFT or K.INK)
        ty = ty + H_SMALL + 3
    end
    local bw, bh = inkButton(self, "Read all notes", px, actionsY, mx, my)
    self.feedHits[#self.feedHits + 1] = { kind = "read", e = e, x = px, y = actionsY, w = bw, h = bh }
    if mx >= px and mx < px + bw and my >= actionsY and my < actionsY + bh then tip(self, "Read all notes\nOpens its full change notes in the history below.") end
    -- an update still waiting to download: Update (PZMM installs it; else Steam's page)
    if e.pending then
        local ux = px + bw + 8
        local uw, uh = inkButton(self, T("feed_update", "Update"), ux, actionsY, mx, my)
        local item = e.item
        self.feedHits[#self.feedHits + 1] = { kind = "fn", fn = function()
            if self.app then self:request("install\t" .. item, "Update asked for.") else self:openWorkshop(item) end
        end, x = ux, y = actionsY, w = uw, h = uh }
        if mx >= ux and mx < ux + uw and my >= actionsY and my < actionsY + uh then
            tip(self, self.app and "Update\nOpens PZMM, which downloads the update once you say yes." or "Update\nSteam downloads it when it next can. Opens its Workshop page.")
        end
    end
    if #e.risks > 0 then
        -- the stamp: blood-red letters in a blood-red frame
        local label = "MAY BREAK SAVES"
        local sw, sh = width(label) + 14, H_SMALL + 6
        local sx, sy = x + w - 20 - sw, actionsY + 2
        outline(self, sx, sy, sw, sh, 3, K.BLOOD, 0.85)
        outline(self, sx + 1, sy + 1, sw - 2, sh - 2, 3, K.BLOOD, 0.85)
        bold(self, label, sx + 7, sy + 3, K.BLOOD)
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
    -- the links a text holds, each on a line of its own (clickable: they open through openLink)
    local function addNoteLinks(raw)
        local seenUrl, nlinks = {}, 0
        local function addLink(title, url)
            if nlinks >= 4 or seenUrl[url] then return end
            seenUrl[url] = true
            nlinks = nlinks + 1
            lines[#lines][4] = 2
            lines[#lines + 1] = { fit(plain(title) ~= "" and plain(title) or url, w), C.accentText, false, 2, "link", url }
        end
        for t, u in raw:gmatch("%[([^%]]*)%]%((https?://[^)%s]+)%)") do addLink(t, u) end
        for u, t in raw:gmatch("%[[Uu][Rr][Ll]=(https?://[^%]]+)%]([^%[]*)%[/[Uu][Rr][Ll]%]") do addLink(t, u) end
        for u in raw:gmatch("%[[Uu][Rr][Ll]%](https?://[^%[]+)%[/[Uu][Rr][Ll]%]") do addLink(u, u) end
        for u in raw:gmatch("https?://[^%s%]%)<>\"]+") do addLink(u, u) end
    end
    add(table.concat(e.mods, ", "), C.muted, false, 8)
    if #e.notes > 0 then
        for i, n in ipairs(e.notes) do
            if i > K.SHOWN_NOTES and not more then break end
            add(date(n.time), C.text, true, 4)
            local body = plain(n.text)
            for _, l in ipairs(wrap(body ~= "" and body or "(no text)", w)) do add(l, C.text, false) end
            -- the links the note holds, each on a line of its own
            addNoteLinks(n.text)
            lines[#lines][4] = 10
        end
        if #e.notes > K.SHOWN_NOTES then
            local rest = #e.notes - K.SHOWN_NOTES
            add(more and "Show fewer" or ("Show " .. rest .. " older note" .. (rest == 1 and "" or "s")), C.accentText, false, 2, "more")
        end
    elseif #e.logs > 0 then
        add("No Workshop change notes. The mod's own changelog, newest first:", C.muted, false, 8)
        for i, l in ipairs(e.logs) do
            if i > K.SHOWN_NOTES and not more then break end
            add(l.title, C.text, true, 4)
            for _, part in ipairs(wrap(plain(l.text), w)) do add(part, C.text, false) end
            addNoteLinks(l.text)
            lines[#lines][4] = 10
        end
        if #e.logs > K.SHOWN_NOTES then
            local rest = #e.logs - K.SHOWN_NOTES
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
function PZMMMods:entryH(e, w)
    if not self.feedOpen[e.item] then return 32 + H_SMALL + 4 end
    local bodyH = 0
    for _, l in ipairs(self:entryLines(e, w - 60)) do bodyH = bodyH + H_SMALL + l[4] end
    return 32 + bodyH + 12
end
function PZMMMods:drawEntry(e, x, y, w, mx, my, top, bottom)
    local open = self.feedOpen[e.item]
    local HEADH = 32
    local lines = open and self:entryLines(e, w - 60) or nil
    local h = self:entryH(e, w)
    if self.feedJump == e.item then
        self.feedScroll = math.max(0, self.feedScroll + (y - top) - 12)
        self.feedJump = nil
    end
    if y + h < top or y > bottom then return y + h end
    local on = self:entryOn(e)
    round(self, x, y, w, h, 6, C.raised)
    outline(self, x, y, w, h, 6, C.line)
    local ty = y + (HEADH - H_SMALL) / 2
    -- right: the Workshop's change notes page, a quiet button
    local link = "Change notes"
    local lw = width(link) + 48
    local bty = y + math.floor((HEADH - BAR) / 2)
    local lx = x + w - 10 - lw
    local _, _, overLink = flatButton(self, link, lx, bty, mx, my, "quiet", TB.search)
    if overLink then tip(self, "Change notes\nIts change notes page on the Workshop, in your browser or the Steam overlay.") end
    self.feedHits[#self.feedHits + 1] = { kind = "url", url = "https://steamcommunity.com/sharedfiles/filedetails/changelog/" .. e.item, x = lx, y = bty, w = lw, h = BAR }
    -- an update still waiting to download: Update (PZMM installs it; else Steam's page)
    local eyeAfter = lx
    if e.pending then
        local label = T("feed_update", "Update")
        local ubw = width(label) + 48
        lx = lx - 8 - ubw
        eyeAfter = lx
        local _, _, over = flatButton(self, label, lx, bty, mx, my, "quiet", TB.update)
        if over then tip(self, self.app and "Update\nOpens PZMM, which downloads the update once you say yes." or "Update\nSteam downloads it when it next can: restarting Steam gets it now. Opens its Workshop page.") end
        local item = e.item
        self.feedHits[#self.feedHits + 1] = { kind = "fn", fn = function()
            if self.app then self:request("install\t" .. item, "Update asked for.") else self:openWorkshop(item) end
        end, x = lx, y = bty, w = ubw, h = BAR }
    end
    -- the eye: show it in My mods
    do
        local ex0 = eyeAfter - 8 - BAR
        local _, eo = toolButton(self, TB.eye, nil, ex0, bty, mx, my)
        if eo then tip(self, "Show it in My mods") end
        self.feedHits[#self.feedHits + 1] = { kind = "show", e = e, x = ex0, y = bty, w = BAR, h = BAR }
        lx = ex0
    end
    -- left: the fold, the title, versions, chips
    local chev = open and K.CHEV_DOWN or K.CHEV_RIGHT
    if chev then
        self:drawTextureScaled(chev, x + 10, y + math.floor(HEADH / 2) - 7, 14, 14, 1, C.muted.r, C.muted.g, C.muted.b)
    else
        text(self, open and "v" or ">", x + 14, ty, C.muted)
    end
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
        local label = T("feed_may_break", "May break saves")
        if TB.alert then
            self:drawTextureScaled(TB.alert, tx, y + math.floor((HEADH - 14) / 2), 14, 14, 1, C.danger.r, C.danger.g, C.danger.b)
            tx = tx + 18
        end
        bold(self, label, tx, ty, C.danger)
        tx = tx + width(label) + 7
        if mx >= cx and mx < tx and my >= y and my < y + HEADH then tip(self, riskTip(e)) end
    end
    if not on then text(self, "Disabled", tx, ty, C.muted) end
    self.feedHits[#self.feedHits + 1] = { kind = "fold", e = e, x = x, y = y, w = w, h = HEADH }
    -- the body; closed, a one-line peek at the newest note
    local by = y + HEADH
    if not lines then
        if e.peekFor ~= e.loaded then
            local body = excerpt(e):match("^[^\n]*") or ""
            if body == "" then body = e.loaded and "No change notes." or "Its change notes are on the Workshop." end
            e.peek, e.peekFor = body, e.loaded
        end
        text(self, fit(e.peek, w - 60), x + 30, y + HEADH, C.muted)
    end
    if lines then
        for _, l in ipairs(lines) do
            local col = l[2]
            if l[5] == "more" then
                local over = mx >= x + 30 and mx < x + 30 + width(l[1]) and my >= by and my < by + H_SMALL
                if over then col = C.text end
                self.feedHits[#self.feedHits + 1] = { kind = "more", e = e, x = x + 30, y = by, w = width(l[1]), h = H_SMALL + 2 }
            end
            local f = l[3] and bold or text
            if l[5] == "link" then
                local lw2 = width(l[1])
                local over = mx >= x + 30 and mx < x + 30 + lw2 and my >= by and my < by + H_SMALL + 2
                f(self, l[1], x + 30, by, over and C.text or col)
                rect(self, x + 30, by + H_SMALL, lw2, 1, over and C.text or col, 1)
                if over then tip(self, l[6]) end
                self.feedHits[#self.feedHits + 1] = { kind = "url", url = l[6], x = x + 30, y = by, w = lw2, h = H_SMALL + 2 }
            else
                f(self, l[1], x + 30, by, col)
            end
            by = by + H_SMALL + l[4]
        end
    end
    return y + h
end

function PZMMMods:drawFeed()
    local bx, by, bw, bh = self:feedBox()
    local cw = math.min(bw, 1180)
    local x0 = math.floor(bx + (bw - cw) / 2)
    local mx, my = self:getMouseX(), self:getMouseY()
    if my < by or my >= by + bh then my = -1e9 end
    self.feedHits = {}
    self:setStencilRect(bx, by, bw, bh)
    local y = by + 8 - self.feedScroll
    local f = self.feed or self:liteFeed()
    -- seen while it's open, however you got here (and updates Steam reports while you're on it)
    self:sawFeed()
    -- the banner first, as on every tab
    if not self.feed then y = self:drawNotice(x0, y, cw, T("notice_feed", "change notes and their history, and loading the notes for more mods."), mx, my) end
    bold(self, T("tab_feed", "What's new"), x0, y, C.text, MEDIUM)
    local rx = x0 + cw
    if f then
        -- Enabled only, at the right of the heading
        local label = T("maps_enabled_only", "Enabled only")
        local cx = x0 + cw - width(label) - BOX - 8
        local cy = y + (H_MEDIUM - BOX) / 2
        checkbox(self, cx, cy, self.enabledOnly)
        text(self, label, cx + BOX + 8, y + (H_MEDIUM - H_SMALL) / 2, C.text)
        self.feedHits[#self.feedHits + 1] = { kind = "only", x = cx - 4, y = cy - 4, w = BOX + 16 + width(label), h = BOX + 8 }
        if mx >= cx - 4 and mx < cx + BOX + 12 + width(label) and my >= cy - 4 and my < cy + BOX + 4 then tip(self, "Enabled only\nHide updates to mods you have turned off.") end
        rx = cx - 16
    end
    -- Refresh: PZMM's feed read again, and Steam asked again which of your mods changed
    do
        local label = T("feed_refresh", "Refresh")
        local bw = width(label) + 48
        local bxr, byr = rx - bw, y + math.floor((H_MEDIUM - BAR) / 2)
        local _, _, over = flatButton(self, label, bxr, byr, mx, my, "quiet", ACTIONS[2][2])
        if over then tip(self, "Refresh\nReads the change notes again and asks Steam which of your Workshop mods changed.") end
        self.feedHits[#self.feedHits + 1] = { kind = "fn", fn = function()
            self.feed, self.steam, self.steamState, self.liteFeedCache = readFeed(), nil, nil, nil
            self:querySteam()
        end, x = bxr, y = byr, w = bw, h = BAR }
    end
    y = y + H_MEDIUM + 4
    if not f then
        self.feedHeight = 0
        text(self, self.steamState == "failed" and "Steam didn't answer. Rescan, at the top right, asks it again." or "Asking Steam which of your Workshop mods were updated...", x0, y, C.muted)
        self:clearStencilRect()
        return
    end
    local since = tonumber(f.since) or 0
    local sub = (f.basis == "launch" and ("Your Workshop mods updated since you last played (" .. date(since) .. ")."))
        or (f.basis == "save" and ("Your Workshop mods updated since your last save (" .. date(since) .. ")."))
        or "Your Workshop mods updated in the last week (no sign of play yet)."
    text(self, sub, x0, y, C.muted)
    y = y + H_SMALL + 16

    local shown, risky = {}, 0
    for _, e in ipairs(f.entries) do
        local on = self:entryOn(e)
        if on or not self.enabledOnly then shown[#shown + 1] = e end
        if on and #e.risks > 0 then risky = risky + 1 end
    end
    if risky > 0 then
        local s = risky == 1 and "1 enabled mod changed in a way that may break a save in progress. Read its notes before you load a save."
            or (risky .. " enabled mods changed in ways that may break a save in progress. Read their notes before you load a save.")
        -- a danger bar at the left, a faint danger fill
        local lines = wrap(s, cw - 48)
        local rh = #lines * (H_SMALL + 2) + 14
        rect(self, x0, y, cw, rh, C.danger, 0.1)
        rect(self, x0, y, 3, rh, C.danger, 1)
        if TB.alert then self:drawTextureScaled(TB.alert, x0 + 16, y + 7, 16, 16, 1, C.danger.r, C.danger.g, C.danger.b) end
        for i, l in ipairs(lines) do text(self, l, x0 + 40, y + 7 + (i - 1) * (H_SMALL + 2), C.text) end
        y = y + rh + 14
    end
    if #shown == 0 then
        text(self, (self.enabledOnly and #f.entries > 0) and "None of your enabled mods were updated since you last played." or "None of your mods were updated since you last played.", x0, y, C.muted)
        y = y + H_SMALL + 14
    end

    -- the board: the latest, as scraps of paper taped on the blueprint
    local cols = math.max(1, math.floor((cw + 34) / 334))
    local cardW = (cw - 34 * (cols - 1)) / cols
    local k = cardW / 300
    local cardH = 230 * k
    local pinned = math.min(#shown, cols * K.ROWS)
    for i = 1, pinned do
        local col, row = (i - 1) % cols, math.floor((i - 1) / cols)
        local cx, cy = x0 + col * (cardW + 34), y + row * (cardH + 30)
        if cy + cardH >= by and cy <= by + bh then self:drawScrap(shown[i], cx, cy, cardW, cardH, k, mx, my) end
    end
    if pinned > 0 then y = y + math.ceil(pinned / cols) * (cardH + 30) + 16 end

    -- the history: every change note, newest day first
    if #shown > 0 then
        -- all in one card: its height first (the card is drawn under its entries)
        local ix, iw = x0 + 18, cw - 36
        local hh, last = 18 + H_MEDIUM + 12, nil
        for _, e in ipairs(shown) do
            local day = hasNotes(e) and date(e.updated, true) or "No change notes"
            if day ~= last then
                last = day
                hh = hh + 6 + H_SMALL + 8
            end
            hh = hh + self:entryH(e, iw) + 8
        end
        hh = hh + 10
        card(self, x0, y, cw, hh)
        local cy0 = y
        y = y + 18
        bold(self, T("feed_history", "All change notes"), ix, y, C.text, MEDIUM)
        y = y + H_MEDIUM + 12
        local lastDay
        for _, e in ipairs(shown) do
            -- the app puts updates with nothing to read last: one heading for them, not their days
            local day = hasNotes(e) and date(e.updated, true) or "No change notes"
            if day ~= lastDay then
                lastDay = day
                y = y + 6
                bold(self, day, ix, y, C.muted)
                y = y + H_SMALL + 8
            end
            y = self:drawEntry(e, ix, y, iw, mx, my, by, by + bh) + 8
        end
        y = cy0 + hh + 18
        for _, l in ipairs(wrap(K.FINE, cw)) do
            text(self, l, x0, y, C.muted)
            y = y + H_SMALL + 2
        end
    end
    self:clearStencilRect()
    self.feedHeight = y + self.feedScroll - by + 24
    self.feedScroll = math.max(0, math.min(self.feedScroll, self.feedHeight - bh))
    scrollbar(self, "feedScroll", bx, by, bw, bh, self.feedHeight)
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
            elseif h.kind == "mapdrag" then
                self.mapDrag = { i = h.i, y0 = y }
            elseif h.kind == "griddrag" then
                if self.gv then self.gridDrag = { x0 = self:getMouseX(), y0 = self:getMouseY(), cx = self.gv.cx, cy = self.gv.cy } end
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


--- Switch PZMM's profile: its profiles to pick from (PZMM confirms, then the game's list follows).
--- One item per PZMM profile (the current one ticked); nil when PZMM hasn't listed any.
function PZMMMods:profileItems()
    local pr = readProfiles()
    if not pr or #pr.list == 0 then return nil end
    local items = {}
    for _, e in ipairs(pr.list) do
        items[#items + 1] = { e.name, function()
            if e.name == pr.active then return end
            self:request("profile\t" .. e.name, "Switching to " .. e.name .. ": say yes in PZMM.")
        end, e.name == pr.active }
    end
    return items
end
function PZMMMods:profileMenu()
    local items = self:profileItems()
    if not items then return self:say("Open PZMM once: it lists your profiles here.") end
    self:menu(items)
end

function PZMMMods:onMouseDown(x, y)
    if barDown(self, x, y) then return true end
    if inside(self.logToggle, x, y) then
        getSoundManager():playUISound("UISelectListItem")
        self.logOpen, self.logScroll = not self.logOpen, 0
        return true
    end
    if inside(self.logCopy, x, y) and #PZMMMods.log > 0 then
        local lines = {}
        for _, e in ipairs(PZMMMods.log) do lines[#lines + 1] = clock(e.time) .. "  " .. (e.from == "app" and "PZMM: " or "") .. e.text end
        pcall(function() Clipboard.setClipboard(table.concat(lines, "\n")) end)
        self:addLog("Activity copied.", "info", "sync")
        return true
    end
    if self.logOpen then
        for _, h in ipairs(self.logHits or {}) do
            if inside(h, x, y) then
                getSoundManager():playUISound("UISelectListItem")
                h.fn()
                return true
            end
        end
    end
    if self.logOpen and inside(self.logBox, x, y) then return true end
    local pb = self.profileBox
    if pb and x >= pb.x and x < pb.x + pb.w and y >= pb.y and y < pb.y + pb.h then
        getSoundManager():playUISound("UISelectListItem")
        if self.app then self:profileMenu() else openLink(K.RELEASES_URL) end
        return true
    end
    for _, a in ipairs(self.actionBoxes or {}) do
        if x >= a[2] and x < a[2] + a[4] and y >= a[3] and y < a[3] + BAR then
            getSoundManager():playUISound("UISelectListItem")
            self:onAction(a[1])
            return true
        end
    end
    for _, l in ipairs(self.linkBoxes or {}) do
        if x >= l[2] and x < l[2] + BAR and y >= l[3] and y < l[3] + BAR then
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
    for _, b in ipairs(self.toolBoxes or {}) do
        if x >= b[2] and x < b[2] + b[4] and y >= b[3] and y < b[3] + b[5] then
            if not b[6] then
                getSoundManager():playUISound("UISelectListItem")
                self:onTool(b[1], b[2], b[3] + BAR + 4)
            end
            return true
        end
    end
    for _, t in ipairs(self.tabBoxes or {}) do
        if x >= t[2] and x < t[2] + t[4] and y >= t[3] and y < t[3] + t[5] then
            self.tab = t[1]
            self.scroll = 0
            self:updateView()
            return true
        end
    end
    for _, bh in ipairs(self.bannerHits or {}) do
        if inside(bh, x, y) then
            getSoundManager():playUISound("UISelectListItem")
            if bh.kind == "url" then openLink(bh.url) else bh.fn() end
            return true
        end
    end
    for _, nh in ipairs(self.noticeHits or {}) do
        if x >= nh.x and x < nh.x + nh.w and y >= nh.y and y < nh.y + nh.h then
            getSoundManager():playUISound("UISelectListItem")
            if nh.kind == "url" then openLink(nh.url) else nh.fn() end
            return true
        end
    end
    local lx, ly, lw, lh = self:listBox()
    local sx, sy, sw, sh = self:sideBox()
    -- the list column clips to the list, the side panel to its own box
    if x < sx then
        if y < ly or y > ly + lh then return true end
    elseif y < sy or y > sy + sh then
        return true
    end
    for _, h in ipairs(self.hits or {}) do
        if inside(h, x, y) then
            getSoundManager():playUISound("UISelectListItem")
            if h.kind == "drag" or h.kind == "dragsec" then
                if self:filtering() then
                    self:say("Clear the search and filters, and sort by library order, to drag: a drop needs the whole list.")
                elseif self.app and not self.lib then
                    self:say("Open PZMM once: it brings your profile's sections here to sort.")
                elseif h.kind == "drag" then
                    self.drag = { mod = h.mod, y0 = y }
                else
                    -- the section after it: dropping there leaves it where it is
                    local next, seen = nil, false
                    for _, e in ipairs(self.entries) do
                        if e.head and seen and not next then next = e.head end
                        if e.head == h.g then seen = true end
                    end
                    self.drag = { sec = h.g, next = next, y0 = y }
                end
            elseif h.kind == "head" then
                -- a double-click on a section's tape renames it
                local t = getTimestampMs()
                if h.tx0 and x >= h.tx0 and x < h.tx1 and self.headAt and self.headG == h.g and t - self.headAt < 400 and (not self.lib or self.lib.header[h.g]) then
                    self.headAt = nil
                    self:renameSection(h.g)
                    return true
                end
                self.headAt, self.headG = t, h.g
                self.collapsed[h.g] = not self.collapsed[h.g]
                K.writeCollapsed(self.collapsed)
                self:updateView()
            elseif h.kind == "box" then
                self:toggle(h.mod)
            elseif h.kind == "orow" then
                self.odrag = { i = h.i, id = h.id, d = h.mod, y0 = y }
            elseif h.kind == "fn" then
                h.fn()
            else
                local ctrl, shift = false, false
                pcall(function() ctrl, shift = isCtrlKeyDown(), isShiftKeyDown() end)
                if (ctrl or shift) and h.mod then
                    if shift and self.anchor then
                        -- a run, from the last one clicked to this one, in the list's order
                        local a, b
                        for i, e in ipairs(self.entries) do
                            if e.mod == self.anchor then a = i end
                            if e.mod == h.mod then b = i end
                        end
                        if a and b then
                            for i = math.min(a, b), math.max(a, b) do
                                local e = self.entries[i]
                                if e.mod then self.picked[e.mod.modId] = true end
                            end
                        end
                    else
                        if self.selected and not self.picked[self.selected.modId] then self.picked[self.selected.modId] = true end
                        self.picked[h.mod.modId] = not self.picked[h.mod.modId] or nil
                    end
                    self.anchor = h.mod
                else
                    self.picked = {}
                    self.anchor = h.mod
                    self.orderSel = nil
                    self.selected = h.mod
                    self.checkOpen = false
                end
            end
            return true
        end
    end
    return true
end

function PZMMMods:onMouseUp(x, y)
    self.sbDrag = nil
    local gd = self.gridDrag
    self.gridDrag = nil
    -- a press on the grid that hardly moved is a click: it picks that cell
    if gd then
        local gx, gy = self:getMouseX(), self:getMouseY()
        if math.abs(gx - gd.x0) < 4 and math.abs(gy - gd.y0) < 4 then self:pickCell(gx, gy) end
    end
    local md = self.mapDrag
    self.mapDrag = nil
    if md and md.moved and md.to and md.to ~= md.i and md.to ~= md.i + 1 then
        self:moveMap(md.i, md.to > md.i and md.to - 1 or md.to)
    end
    local od = self.odrag
    self.odrag = nil
    if od then
        if od.moved and od.to and od.to ~= od.i and od.to ~= od.i + 1 then
            self:remember()
            local list = self:activeOrder()
            local id = table.remove(list, od.i)
            local at = od.to > od.i and od.to - 1 or od.to
            table.insert(list, at, id)
            self:setOrder(list, ((od.d and od.d.name) or id) .. " now loads " .. at .. ((at % 100 >= 11 and at % 100 <= 13) and "th." or at % 10 == 1 and "st." or at % 10 == 2 and "nd." or at % 10 == 3 and "rd." or "th."))
        elseif not od.moved then
            -- the first click highlights the row; a second within 400 ms opens its details
            local t = getTimestampMs()
            if od.d and self.orderSel == od.id and self.orderClickAt and t - self.orderClickAt < 400 then
                self.selected = od.d
                self.checkOpen = false
            end
            self.orderSel, self.orderClickAt = od.id, t
        end
        return true
    end
    local dr = self.drag
    self.drag = nil
    -- a real move makes the game's own sections first, if there are none yet
    if dr and dr.moved and dr.target and self:ensureLibrary() then
        if dr.sec then self:placeSection(dr.sec, dr.target.before) else self:placeMod(dr.mod, dr.target.g, dr.target.before) end
    end
    return true
end
function PZMMMods:onMouseUpOutside(x, y)
    self.drag, self.odrag, self.sbDrag, self.mapDrag, self.gridDrag = nil, nil, nil, nil, nil
    return true
end

function PZMMMods:onRightMouseDown(x, y)
    self.drag, self.odrag = nil, nil
    if self.page ~= "mods" then return true end
    for _, h in ipairs(self.hits or {}) do
        if (h.kind == "row" or h.kind == "head" or h.kind == "orow") and inside(h, x, y) then
            if h.kind == "head" then
                self:sectionMenu(h.g)
            elseif h.kind == "orow" then
                -- a load order row: highlighted, and its mod's menu
                self.orderSel = h.id
                if h.mod then self:modMenu(h.mod) end
            else
                self:modMenu(h.mod)
            end
            return true
        end
    end
    return true
end
--- Several picked mods' menu: what it does, it does to all of them.
function PZMMMods:manyMenu(list)
    local n = #list
    local favs, hid = true, true
    for _, m in ipairs(list) do
        if not m.favorite then favs = false end
        if not self.hidden[m.modId] then hid = false end
    end
    local items = {
        { "Enable " .. n .. " mods", function() self:onTool("bulkOn") end },
        { "Disable " .. n .. " mods", function() self:onTool("bulkOff") end },
        { favs and "Unfavourite" or "Favourite", function()
            for _, m in ipairs(list) do self.model:setFavorite(m.modId, not favs) end
            self.model:refreshMods()
        end },
        false,
        { "Copy mod IDs", function()
            local ids = {}
            for _, m in ipairs(list) do ids[#ids + 1] = m.modId end
            Clipboard.setClipboard(table.concat(ids, ";"))
            self:say(n .. " mod IDs copied.")
        end },
        { T("menu_top", "Move to top"), function() self:moveMany(PZMMMods.MOVE_TOP) end },
        { T("menu_bottom", "Move to bottom"), function() self:moveMany(PZMMMods.MOVE_BOTTOM) end },
        false,
        { hid and "Show again" or "Hide", function()
            for _, m in ipairs(list) do self.hidden[m.modId] = (not hid) or nil end
            writeHidden(self.hidden)
            self.picked = {}
            self:updateView()
            self:say(hid and "Shown again." or "Hidden: Filters > Hidden shows them.")
        end },
    }
    self:menu(items)
end

--- Maps, with this map mod picked in its list.
function PZMMMods:showInMaps(d)
    self.page, self.feedScroll = "maps", 0
    self.mapQuery, self.mapFilter, self.gridSel = nil, "all", nil
    self.cellVer = (self.cellVer or 0) + 1
    for _, m in ipairs(self:mapRows()) do
        if m.d == d then self.mapSel = m end
    end
end

--- A mod's menu: Enable or Disable, favourite, its ID, where it goes in the list, its Workshop page, its files, Show in
--- Maps for a map. On a mod that's one of two or more picked, it acts on all of them.
function PZMMMods:modMenu(d)
    if d.ghost then
        return self:menu({
            { "Disable", function() self:toggle(d) end },
            false,
            { "Copy mod ID", function() Clipboard.setClipboard(d.modId); self:say(d.modId .. " copied.") end },
        })
    end
    local np = 0
    for _ in pairs(self.picked) do np = np + 1 end
    if np >= 2 and self.picked[d.modId] then
        local list = {}
        for id in pairs(self.picked) do
            local m = self.model.mods[id]
            if m then list[#list + 1] = m end
        end
        table.sort(list, byName)
        if #list > 1 then return self:manyMenu(list) end
    end
    local ws = wsOf(d)
    local items = {
        { d.isActive and "Disable" or "Enable", function() self:toggle(d) end },
        { d.favorite and "Unfavourite" or "Favourite", function()
            self.model:setFavorite(d.modId, not d.favorite)
            self.model:refreshMods()
        end },
        false,
        { "Copy mod ID", function()
            Clipboard.setClipboard(d.modId)
            self:say(d.modId .. " copied.")
        end },
        { T("menu_top", "Move to top"), function() self:moveOne(d, PZMMMods.MOVE_TOP) end },
        { T("menu_bottom", "Move to bottom"), function() self:moveOne(d, PZMMMods.MOVE_BOTTOM) end },
    }
    if ws and ws ~= "" and ws ~= "0" then
        items[#items + 1] = { "Workshop page", function() self:openWorkshop(ws) end }
    end
    items[#items + 1] = { "Show files", function() pcall(function() showFolderInDesktop(d.modInfo:getDir()) end) end }
    local isMap = d.category == "map" or (self.mapScanState and self.mapScanState.modOf[d.modId])
    if isMap then items[#items + 1] = { T("menu_maps", "Show in Maps"), function() self:showInMaps(d) end } end
    local hid = self.hidden[d.modId]
    items[#items + 1] = false
    items[#items + 1] = { hid and "Show again" or "Hide", function()
        self.hidden[d.modId] = (not hid) or nil
        writeHidden(self.hidden)
        self:updateView()
        self:say(hid and (d.name .. " shows again.") or (d.name .. " hidden: Filters > Hidden shows it."))
    end }
    self:menu(items)
end

function PZMMMods:onMouseWheel(del)
    if self.logOpen and inside(self.logBox, self:getMouseX(), self:getMouseY()) then
        self.logScroll = math.max(0, math.min((self.logScroll or 0) - del * 48, (self.logFull or 0) - (self.logBox.view or self.logBox.h)))
        return true
    end
    if self.page == "maps" then
        local mx, my = self:getMouseX(), self:getMouseY()
        -- (the grid's and legend's boxes are cut to the page's frame: not under the header or footer)
        if self.gv and inside(self.gridHit, mx, my) then
            -- the wheel zooms the grid about the mouse
            self:zoomGrid(del < 0 and 1.25 or 0.8, mx, my)
            return true
        end
        if self.legendBox and inside(self.legendBox, mx, my) then
            self.legendScroll = (self.legendScroll or 0) + del * 48
            return true
        end
    end
    if self.page == "home" and self.libBox and inside(self.libBox, self:getMouseX(), self:getMouseY()) then
        self.libScroll = math.max(0, math.min((self.libScroll or 0) + del * 48, self.libBox.full - self.libBox.h))
        return true
    end
    if self.page ~= "mods" then
        local _, _, _, bh = self:feedBox()
        if self.page == "home" then bh = self.height - BOTTOM - TOP end
        self.feedScroll = math.max(0, math.min(self.feedScroll + del * 48, (self.feedHeight or 0) - bh))
        return true
    end
    local db = self.detailBox
    if (self.selected or self.checkOpen) and db and inside(db, self:getMouseX(), self:getMouseY()) then
        self.detailScroll = math.max(0, math.min((self.detailScroll or 0) + del * 48, db.full - db.h))
        return true
    end
    local o = self.orderBox
    if not (self.selected or self.checkOpen) and o and inside(o, self:getMouseX(), self:getMouseY()) then
        self.orderScroll = math.max(0, math.min((self.orderScroll or 0) + del * 48, o.full - o.h))
        return true
    end
    local _, _, _, lh = self:listBox()
    self.scroll = math.max(0, math.min(self.scroll + del * 48, (self.listHeight or 0) - lh))
    return true
end

-- --- actions

function PZMMMods:close()
    -- a loud request PZMM hasn't taken and isn't running for: start it now, the screen won't be here later
    local loud = self.launchAt ~= nil
    for _ in pairs(self.sent or {}) do loud = true end
    if loud and not self:appUp() then self:launchNow() end
    self.launchAt, self.drag = nil, nil
    self:setVisible(false)
    self:removeFromUIManager()
    if MainScreen.instance and MainScreen.instance.bottomPanel then MainScreen.instance.bottomPanel:setVisible(true) end
end

function PZMMMods:onAccept()
    if self.defChanged then
        self:ask({ title = "PZMM changed your mods since you opened this screen. Accepting writes this screen's list over them.", choices = {
            { "Accept anyway", function() self:acceptNow() end },
            { "Use PZMM's mods", function() self:takeAppMods() end },
        } })
        return
    end
    self:acceptNow()
end
--- PZMM's mods on and off here as its profile has them (library.txt's +/- lines; the game can't read
--- default.txt again), not saved, and that state is the new baseline: nothing to accept, nothing to undo.
--- Forget everything worked out from the mod tables (a reload makes new ones).
function PZMMMods:resetCaches()
    self.mapOrder, self.saves, self.java, self.selected, self.liteFeedCache = nil, nil, nil, nil, nil
    self.cats, self.neededAll = nil, nil
    self.mapScanState, self.gridCache, self.mapRowsKey, self.legendKey, self.langDone = nil, nil, nil, nil, false
    self.mapRowsList, self.legendList = nil, nil
    self.mapSel, self.mapClash, self.gridSel, self.cellVer = nil, nil, nil, (self.cellVer or 0) + 1
end
function PZMMMods:takeAppMods()
    local lib = readLibrary()
    if not lib then return end
    -- the game's list first as the profile has it saved (the profile-switch path does the same), then library.txt's +/-
    pcall(function() self.model:reloadMods() end)
    self:resetCaches()
    for _, l in ipairs(lib.lines) do
        local sign, id = l:match("^([+-])(.+)$")
        local d = id and self.model.mods[id]
        if d and d.isAvailable and d.isActive ~= (sign == "+") then
            pcall(function() self.model:setModActive(id, sign == "+") end)
            d.isActive = sign == "+"
        end
    end
    -- and PZMM's load order (order.txt, its enabled mods top to bottom): a mod turned on here would go last
    pcall(function()
        local r = getFileReader("pzmm/order.txt", false)
        if not r then return end
        local list, seen, line = {}, {}, r:readLine()
        while line do
            if line ~= "" and self.model.mods[line] and self.model.mods[line].isActive and not seen[line] then
                list[#list + 1], seen[line] = line, true
            end
            line = r:readLine()
        end
        r:close()
        for _, id in ipairs(self:activeOrder()) do
            if not seen[id] then list[#list + 1] = id end
        end
        self:setOrder(list)
    end)
    self.model:refreshMods()
    for _, d in pairs(self.model.mods) do d.defaultActive = d.isActive end
    pcall(function()
        local o = ActiveMods.getById("pzmmUndo")
        o:copyFrom(self.model:getActiveMods())
        self.modsOrig = o
    end)
    self.undos, self.redos, self.diffAt = {}, {}, nil
    self.defRev, self.defChanged = self.app and self.app.default_rev, nil
    self.lib = lib
    self:updateView()
end

function PZMMMods:acceptNow()
    self:close()
    self.model:acceptChanges() -- saves (and restarts the game's Lua when the mod set needs it)
end

--- The game's presets file, with the favourites as they were when this screen opened (its saveModDataToFile
--- writes the favourites on screen, which would save toggles you haven't accepted).
function PZMMMods:savePresets()
    local keep = {}
    for id, d in pairs(self.model.mods) do keep[id], d.favorite = d.favorite, d.defaultFav end
    pcall(function() self.model:saveModDataToFile() end)
    for id, d in pairs(self.model.mods) do d.favorite = keep[id] end
end

--- Back: changes not saved yet get the game's own question (save them or not).
function PZMMMods:onBack()
    if self:changes(true) > 0 then
        self:ask({ title = "Accept your changes to your mods before you go back?", choices = {
            { "Accept", function() self:onBackAnswer({ internal = "YES" }) end },
            { "Discard", function() self:onBackAnswer({ internal = "NO" }) end },
        } })
        return
    end
    self:close()
end
function PZMMMods:onBackAnswer(b)
    if b.internal == "YES" then return self:onAccept() end
    -- Discard: put everything back in memory and leave without acceptChanges (it would write default.txt)
    if self.modsOrig then
        pcall(function() self.model:getActiveMods():copyFrom(self.modsOrig) end)
        self.model:refreshMods()
    end
    for id, d in pairs(self.model.mods) do
        if d.isActive ~= d.defaultActive then self.model:forceActivateMods(d.modInfo, d.defaultActive) end
        if self.model.favs[id] ~= d.defaultFav then self.model.favs[id] = d.defaultFav end
    end
    self.model:refreshMods() -- d.favorite and d.isActive follow
    self:close()
end

local menuClick = MainScreen.onMenuItemMouseDownMainMenu

PZMMOverlay = ISPanel:derive("PZMMOverlay")

--- Ask, as the app does: `q` = { title, lines = {...}, choices = { { label, fn }, ... } }. The first
--- choice is red; Cancel (or a click outside) closes it.
function PZMMMods:ask(q)
    PZMMOverlay.show({ ask = q })
end
--- A popup menu at the mouse, or at `x`, `y`: `items` = { { label, fn, ticked } or false (a line), ... }.
K.MENU_ICONS = { { "enable", "check" }, { "disable", "x" }, { "unfavourite", "star" }, { "favourite", "star" }, { "remove favourite", "star" },
    { "copy", "copy" }, { "workshop page", "steam" }, { "show files", "folder" }, { "open folder", "folder" }, { "show again", "eye" },
    { "hide", "filter" }, { "move to top", "chev_up" }, { "move to bottom", "chev_down" }, { "delete", "trash" }, { "remove", "trash" },
    { "rename", "pencil" }, { "show in maps", "layers" }, { "switch profile", "swap" }, { "new game", "plus" }, { "continue", "play" },
    { "join", "play" }, { "load", "folder" } }
--- A menu item's icon, by what its English label starts with (nil: none).
function K.menuIcon(label)
    local l = tostring(K.EN[label] or label):lower()
    for _, m in ipairs(K.MENU_ICONS) do
        if l:sub(1, #m[1]) == m[1] then return TB[m[2]] end
    end
    return nil
end
function PZMMMods:menu(items, x, y)
    PZMMOverlay.show({ menu = items, x0 = x and (self:getAbsoluteX() + x) or getMouseX(), y0 = y and (self:getAbsoluteY() + y) or getMouseY() })
end

function PZMMOverlay.show(o)
    local p = PZMMOverlay:new(0, 0, getCore():getScreenWidth(), getCore():getScreenHeight())
    setmetatable(p, PZMMOverlay)
    PZMMOverlay.__index = PZMMOverlay
    for k, v in pairs(o) do p[k] = v end
    PZMMOverlay.current = p
    p.background = false
    p:initialise()
    p:addToUIManager()
    p:setAlwaysOnTop(true)
    p:bringToTop()
    if p.ask and p.ask.input then
        local e = ISTextEntryBox:new(p.ask.input, 0, 0, 300, BAR - 4)
        e.font = SMALL
        e:initialise()
        e:instantiate()
        e.backgroundColor = copy(C.raised, 0)
        e.borderColor = copy(C.line, 0)
        if p.ask.max then pcall(function() e:setMaxTextLength(p.ask.max) end) end
        if p.ask.mask then pcall(function() e:setMasked(true) end) end
        K.tint(e)
        -- Enter: the first choice
        e.onCommandEntered = function() p:choose(p.ask.choices[1][2]) end
        p:addChild(e)
        p.entry = e
        e:focus()
    end
end
--- Close, then run a choice with the text typed (if there's a box for it).
function PZMMOverlay:choose(fn)
    local typed = self.entry and self.entry:getText()
    self:closeIt()
    if fn then fn(typed) end
end
function PZMMOverlay:closeIt()
    -- (the key that closed it must not also reach the screen: onKey ignores keys for a moment)
    PZMMMods.overlayShut = getTimestampMs()
    self:setVisible(false)
    self:removeFromUIManager()
    if PZMMOverlay.current == self then PZMMOverlay.current = nil end
end

function PZMMOverlay:prerender()
    local mx, my = self:getMouseX(), self:getMouseY()
    self.hits = {}
    if self.ask then
        local q = self.ask
        rect(self, 0, 0, self.width, self.height, hex("000000"), 0.5)
        local w = math.min(q.checks and 640 or 560, self.width - 32)
        local tw = w - 44
        local title = wrap(q.title, tw)
        -- a checklist: a capped, scrolling list of ticks under the lines
        local RH = math.max(BOX, H_SMALL) + 10
        local shown = q.checks and math.max(1, math.min(#q.checks, math.floor(self.height * 0.5 / RH))) or 0
        local listH = shown * RH
        local lines = {}
        for _, l in ipairs(q.lines or {}) do
            for i, r in ipairs(wrap(l, tw - 18)) do lines[#lines + 1] = { r, i == 1 } end
        end
        local h = 20 + #title * (H_SMALL + 2) + 10 + (#lines > 0 and #lines * (H_SMALL + 2) + 8 or 0) + (q.checks and listH + 14 or 0) + (self.entry and BAR + 14 or 0) + BAR + 20
        local x, y = math.floor((self.width - w) / 2), math.floor((self.height - h) / 2)
        self.box = { x = x, y = y, w = w, h = h }
        round(self, x - 1, y + 1, w + 2, h + 4, 10, C.black, 0.28 * SHADOW.k)
        round(self, x - 2, y + 2, w + 4, h + 8, 10, C.black, 0.14 * SHADOW.k)
        round(self, x, y, w, h, 10, C.panel)
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
        if q.checks then
            local full = #q.checks * RH
            self.checkScroll = math.max(0, math.min(self.checkScroll or 0, full - listH))
            self.checkBox = { x = x + 12, y = ty, w = w - 24, h = listH, full = full, row = RH }
            round(self, x + 12, ty, w - 24, listH, 6, C.raised, 0.5)
            self:setStencilRect(x + 12, ty, w - 24, listH)
            for i, c in ipairs(q.checks) do
                local ry = ty + (i - 1) * RH - self.checkScroll
                if ry + RH > ty and ry < ty + listH then
                    local over = mx >= x + 12 and mx < x + w - 12 and my >= math.max(ry, ty) and my < math.min(ry + RH, ty + listH)
                    if over then rect(self, x + 12, ry, w - 24, RH, C.hover, 1) end
                    checkbox(self, x + 22, ry + math.floor((RH - BOX) / 2), c.on)
                    local nw = c.note and (width(c.note) + 16) or 0
                    text(self, fit(c.label, w - 24 - 20 - BOX - 10 - nw - 10), x + 22 + BOX + 10, ry + math.floor((RH - H_SMALL) / 2), c.on and C.text or C.muted)
                    if c.note then text(self, c.note, x + w - 22 - width(c.note), ry + math.floor((RH - H_SMALL) / 2), C.accentText) end
                    if over and c.tip then tip(self, c.tip) end
                    local y0, y1 = math.max(ry, ty), math.min(ry + RH, ty + listH)
                    self.hits[#self.hits + 1] = { x = x + 12, y = y0, w = w - 24, h = y1 - y0, keep = true, fn = function() c.on = not c.on; if q.recount then q.recount() end end }
                end
            end
            self:clearStencilRect()
            if full > listH then
                -- where the list is: a thin thumb at its right
                local th = math.max(16, math.floor(listH * listH / full))
                local tyy = ty + math.floor((listH - th) * self.checkScroll / (full - listH))
                round(self, x + w - 17, tyy, 4, th, 2, C.muted, 0.6)
            end
            ty = ty + listH + 14
        end
        if self.entry then
            -- the text box, drawn as the search box is
            round(self, x + 22, ty, w - 44, BAR, 6, C.raised)
            outline(self, x + 22, ty, w - 44, BAR, 6, C.focus)
            outline(self, x + 21, ty - 1, w - 42, BAR + 2, 6, C.focus)
            self.entry:setX(x + 30)
            self.entry:setY(ty + 4)
            self.entry:setWidth(w - 60)
            ty = ty + BAR + 14
        end
        -- the choices at the right: the first red, the rest raised, Cancel quiet and last
        local row = {}
        for i, c in ipairs(q.choices or {}) do row[#row + 1] = { c[1], i == 1 and "primary" or nil, c[2], c[3] } end
        row[#row + 1] = { q.cancel or "Cancel", "quiet" }
        local bx = x + w - 22
        for i = #row, 1, -1 do
            local b = row[i]
            local bw = width(b[1]) + 28 + (b[4] and 20 or 0)
            bx = bx - bw
            flatButton(self, b[1], bx, ty, mx, my, b[2], b[4])
            self.hits[#self.hits + 1] = { x = bx, y = ty, w = bw, h = BAR, fn = b[3] or false }
            bx = bx - 8
        end
    elseif self.menu then
        local IH, HH = H_SMALL + 12, H_SMALL + 10 -- an item's height, and a heading's
        local w = 0
        local ticks = false
        for _, it in ipairs(self.menu) do
            if it then w = math.max(w, width(it.heading and it[1]:upper() or it[1])) end
            if it and it[3] ~= nil then ticks = true end
        end
        local icons = false
        for _, it in ipairs(self.menu) do
            if it and not it.heading then
                it.ic = it.icon or K.menuIcon(it[1])
                if it.ic then icons = true end
            end
        end
        local ind = (ticks or icons) and 24 or 0
        w = math.max(240, w + 28 + ind)
        local h = 8
        for _, it in ipairs(self.menu) do h = h + (it and (it.heading and HH or IH) or 9) end
        local x = math.min(self.x0, self.width - w - 4)
        local y = math.min(self.y0, self.height - h - 4)
        self.box = { x = x, y = y, w = w, h = h }
        round(self, x - 1, y + 1, w + 2, h + 4, 6, C.black, 0.28 * SHADOW.k)
        round(self, x - 2, y + 2, w + 4, h + 8, 6, C.black, 0.14 * SHADOW.k)
        round(self, x, y, w, h, 6, C.raised)
        outline(self, x, y, w, h, 6, C.line)
        local iy = y + 4
        -- the mouse picks an item only when it moves, so the arrow keys can move the pick
        local moved = mx ~= self.lmx or my ~= self.lmy
        self.lmx, self.lmy = mx, my
        for _, it in ipairs(self.menu) do
            if it and it.heading then
                bold(self, it[1]:upper(), x + 14, iy + (HH - H_SMALL) / 2 + 2, C.muted)
                iy = iy + HH
            elseif it then
                local idx = #self.hits + 1
                local off = it.disabled
                if not off and moved and mx >= x and mx < x + w and my >= iy and my < iy + IH then self.sel = idx end
                local lit = not off and self.sel == idx
                if lit then
                    round(self, x + 4, iy, w - 8, IH, 4, C.hover)
                    outline(self, x + 4, iy, w - 8, IH, 4, C.accent, 0.7)
                end
                local tc = it.danger and C.danger or (lit and C.onSelected or C.text)
                local a = off and 0.5 or 1
                -- a tick is true, or a function that says (a menu that stays open follows what you click)
                local ticked = it[3]
                if type(ticked) == "function" then ticked = ticked() end
                local ic = it.ic
                if ticked ~= nil then ic = ticked and TB.check or nil end
                if ic then self:drawTextureScaled(ic, x + 14, iy + math.floor((IH - 14) / 2), 14, 14, a, tc.r, tc.g, tc.b) end
                self:drawText(it[1], x + 14 + ind, iy + 6, tc.r, tc.g, tc.b, a, SMALL)
                if not off then self.hits[idx] = { x = x, y = iy, w = w, h = IH, fn = it[2], keep = it.keep } end
                iy = iy + IH
            else
                rect(self, x + 6, iy + 4, w - 12, 1, C.line, 1)
                iy = iy + 9
            end
        end
    end
end

--- The wheel scrolls an ask's checklist.
function PZMMOverlay:onMouseWheel(del)
    local b = self.checkBox
    if not b then return false end
    self.checkScroll = math.max(0, math.min((self.checkScroll or 0) + del * b.row * 3, b.full - b.h))
    return true
end

function PZMMOverlay:onMouseDown(x, y)
    for _, h in ipairs(self.hits or {}) do
        if x >= h.x and x < h.x + h.w and y >= h.y and y < h.y + h.h then
            getSoundManager():playUISound("UISelectListItem")
            if h.keep then
                -- a toggle in a menu that stays open
                if h.fn then h.fn() end
            else
                self:choose(h.fn)
            end
            return true
        end
    end
    -- a click outside the box cancels; inside it does nothing
    local b = self.box
    if not (b and x >= b.x and x < b.x + b.w and y >= b.y and y < b.y + b.h) then self:closeIt() end
    return true
end
--- Menus: Up and Down pick, Enter chooses. Asks: Enter is the first choice (a box handles its own Enter).
function PZMMOverlay:onKey(key)
    if key == Keyboard.KEY_RETURN then
        if self.menu then
            local h = self.hits and self.hits[self.sel or 0]
            if h and h.keep then
                if h.fn then h.fn() end
            elseif h then
                self:choose(h.fn)
            end
        elseif self.ask and not self.entry then
            local c = self.ask.choices and self.ask.choices[1]
            self:choose(c and c[2])
        end
    elseif self.menu and #(self.hits or {}) > 0 and (key == Keyboard.KEY_UP or key == Keyboard.KEY_DOWN) then
        local n = #self.hits
        local d = key == Keyboard.KEY_DOWN and 1 or -1
        self.sel = ((self.sel or (d > 0 and 0 or 1)) - 1 + d) % n + 1
    end
end
function PZMMOverlay:onRightMouseDown(x, y)
    self:closeIt()
    return true
end

--- Turn a mod on or off. Off, with enabled mods that need it: ask whether they go off too (the app's
--- "Disable them too").
function PZMMMods:toggle(d)
    if d.ghost then
        -- not installed: Disable takes it out of the active list
        self:remember()
        pcall(function() self.model:getActiveMods():setModActive(d.modId, false) end)
        self.ghosts[d.modId] = nil
        if self.selected == d then self.selected = nil end
        self:updateView()
        return
    end
    if not d.isActive then
        local snap = self:snapshot()
        self.model:forceActivateMods(d.modInfo, true)
        if not d.isActive then
            self:say(d.name .. (d.isAvailable and " can't go on: an enabled mod it's incompatible with is on." or " can't load on this build of the game."))
        else
            self:remember(snap)
        end
        return
    end
    -- the enabled mods that need it, and those that need them (the game turns them all off with it)
    local users, seen, todo = {}, { [d.modId] = true }, { d.modId }
    while #todo > 0 do
        local id = table.remove(todo)
        for _, m in pairs(self.model.mods) do
            if m.isActive and not seen[m.modId] and m.requireMods and m.requireMods[id] then
                seen[m.modId] = true
                users[#users + 1] = m
                todo[#todo + 1] = m.modId
            end
        end
    end
    table.sort(users, byName)
    if #users == 0 then
        self:remember()
        self.model:forceActivateMods(d.modInfo, false)
        return
    end
    local list = {}
    for _, m in ipairs(users) do list[#list + 1] = m.name end
    self:ask({ title = (#users == 1 and "This enabled mod needs " or "These enabled mods need ") .. d.name .. ":", lines = list, choices = {
        { "Disable " .. (#users == 1 and "it" or "them") .. " too", function()
            self:remember()
            self.model:forceActivateMods(d.modInfo, false)
            for _, m in ipairs(users) do self.model:forceActivateMods(m.modInfo, false) end
        end },
        { "Only " .. d.name, function()
            -- the game's own off turns off what needs it too: this one alone
            self:remember()
            self.model:setModActive(d.modId, false)
            d.isActive = false
            self.model:refreshMods()
        end },
    } })
end

--- Steam's details for a mod's Workshop item (nil until Steam answers, or for local mods).
function PZMMMods:steamOf(d)
    if not self.steam then return nil end
    local ws = wsOf(d)
    local st = ws and self.steam[ws]
    if st then
        -- a mod PZMM installed, with a Workshop update newer than the install: an update waiting, as Steam's NeedsUpdate
        if st.raw == nil then st.raw = st.state or false end
        local at = self.installed and self.installed[d.modId]
        if not K.STEAM_STATES[st.raw or ""] and at and (st.updated or 0) > at then st.state = "NeedsUpdate"; st.pzmm = true else st.state = st.raw or nil; st.pzmm = nil end
    end
    return st
end

--- Ask Steam about every item you're subscribed to (once per visit): title, update time, download state
--- and the items it requires. Required items you aren't subscribed to are asked about next, for titles.
function PZMMMods:querySteam()
    self.steam = self.steam or {}
    -- "none": nothing subscribed (What's new is empty, not waiting); "asking" until Steam answers
    self.steamState = "none"
    pcall(function()
        local ids = getSteamWorkshopItemIDs()
        local list, seen = ArrayList.new(), {}
        if ids then
            for i = 0, ids:size() - 1 do
                local w = tostring(ids:get(i))
                if not seen[w] then seen[w] = true; list:add(w) end
            end
        end
        -- items PZMM installed from (they aren't subscribed): asked too, for their updates
        for _, w in pairs(installedItem) do
            if w ~= "" and not seen[w] then seen[w] = true; list:add(w) end
        end
        if not list:isEmpty() then
            self.steamState = "asking"
            querySteamWorkshopItemDetails(list, PZMMMods.onSteamDetails, self)
        end
    end)
end
function PZMMMods:onSteamDetails(status, info)
    if status ~= "Completed" then
        if self.steamState == "asking" then self.steamState = "failed" end
        return
    end
    self.steamState = "done"
    local subscribed, wanted = {}, {}
    pcall(function()
        local ids = getSteamWorkshopItemIDs()
        for i = 0, ids:size() - 1 do subscribed[tostring(ids:get(i))] = true end
    end)
    for i = 0, info:size() - 1 do
        pcall(function()
            local det = info:get(i)
            local id = det:getIDString()
            local e = { title = det:getTitle(), updated = tonumber(tostring(det:getTimeUpdated())) or 0, missing = {}, size = 0, created = 0 }
            pcall(function() e.created = tonumber(tostring(det:getTimeCreated())) or 0 end)
            pcall(function() e.size = tonumber(tostring(det:getFileSize())) or 0 end)
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
    if self.sortBy == "size" or self.sortBy == "changed" then self:updateView() end
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
    if self.steamState ~= "done" and self.steamState ~= "none" then return nil end
    local since, basis = self.lastPlayed, "launch"
    if not since then since, basis = os.time() - 7 * 86400, "" end
    local mods = {}
    for id, d in pairs(self.model.mods) do
        local ws = wsOf(d)
        if ws and ws ~= "" then
            mods[ws] = mods[ws] or {}
            table.insert(mods[ws], id)
        end
    end
    for _, ids in pairs(mods) do table.sort(ids) end
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

local B64 = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"
--- Standard base64 (the app's btoa) of an ASCII string.
local function base64(s)
    local out = {}
    for i = 1, #s, 3 do
        local a, b, c = s:byte(i, i + 2)
        local n = a * 65536 + (b or 0) * 256 + (c or 0)
        local q = { math.floor(n / 262144) % 64, math.floor(n / 4096) % 64, math.floor(n / 64) % 64, n % 64 }
        for k = 1, 4 do
            out[#out + 1] = (k <= 2 or (k == 3 and b) or (k == 4 and c)) and B64:sub(q[k] + 1, q[k] + 1) or "="
        end
    end
    return table.concat(out)
end

--- A JSON string literal, ASCII only (anything else \uXXXX).
local function jstr(s)
    s = tostring(s)
    local out = {}
    for i = 1, #s do
        local ch = s:sub(i, i)
        local b = s:byte(i)
        if ch == '"' then out[i] = '\\"'
        elseif ch == "\\" then out[i] = "\\\\"
        elseif b < 32 or b > 126 then out[i] = "\\u" .. string.format("%04x", b) -- UTF-16 units: surrogates come as two
        else out[i] = ch end
    end
    return '"' .. table.concat(out) .. '"'
end

--- The app's share code for the enabled mods: "PZMM1:" + base64 of { name, kind, list, order, workshop, maps }.
function PZMMMods:shareCode()
    local order, list, ws, ids = self:activeOrder(), {}, {}, {}
    for _, id in ipairs(order) do
        list[#list + 1] = "+" .. id .. "\n"
        ids[#ids + 1] = jstr(id)
        local d = self.model.mods[id]
        local w
        if d then w = wsOf(d) end
        if w and w ~= "" and w ~= "0" then ws[#ws + 1] = jstr(id) .. ":" .. jstr(w) end
    end
    local maps = {}
    pcall(function()
        local mo = self.model:getActiveMods():getMapOrder()
        for i = 0, mo:size() - 1 do maps[#maps + 1] = jstr(mo:get(i)) end
    end)
    local json = '{"name":' .. jstr((self.lib and self.lib.profile) or "PZMM") .. ',"kind":"","list":' .. jstr(table.concat(list))
        .. ',"order":[' .. table.concat(ids, ",") .. '],"workshop":{' .. table.concat(ws, ",") .. '},"maps":[' .. table.concat(maps, ",") .. "]}"
    return "PZMM1:" .. base64(json), #order
end

function PZMMMods:onAction(id)
    if id == "share" then
        self:menu({
            { "Copy as a Mod Manager preset", function() self:onAction("sharePreset") end },
            { "Copy as a PZMM share code", function()
                local code, n = self:shareCode()
                pcall(function() Clipboard.setClipboard(code) end)
                self:say(n .. " enabled mods copied as a share code: paste it into PZMM's Import profile.")
            end },
        }, self:getMouseX(), self:getMouseY())
    elseif id == "sharePreset" then
        -- the game's own preset text (its mod screen's Presets > Add takes it): name, then ids, ";" after each
        local ids = {}
        for mid, d in pairs(self.model.mods) do
            if d.isActive then ids[#ids + 1] = mid .. ";" end
        end
        table.sort(ids)
        Clipboard.setClipboard((((self.lib and self.lib.profile) or "PZMM"):gsub(":", "-")) .. ":" .. table.concat(ids) .. "\n")
        self:say(#ids .. " enabled mods copied: paste them into the game's Presets > Add, or send them to a friend.")
    elseif id == "refresh" and self:changes() > 0 and not self.rescanOk then
        self:ask({ title = "Rescan drops changes you haven't accepted. Rescan anyway?", choices = {
            { "Rescan", function() self.rescanOk = true; self:onAction("refresh"); self.rescanOk = nil end },
        } })
    elseif id == "refresh" then
        self.lib, self.app, self.feed = self:readLib(), readApp(), readFeed()
        applyTheme(self.app)
        self:resetCaches()
        -- reloadMods takes what's on now as saved and reads favourites again: keep the unsaved ones
        local was, favs = {}, {}
        for id, d in pairs(self.model.mods) do was[id] = { d.defaultActive, d.defaultFav } end
        for id, v in pairs(self.model.favs or {}) do favs[id] = v end
        self:loadRuleFile()
        self.model:reloadMods()
        for id, v in pairs(favs) do self.model.favs[id] = v end
        for id, d in pairs(self.model.mods) do
            if was[id] then d.defaultActive, d.defaultFav = was[id][1], was[id][2] end
        end
        self.model:refreshMods()
        self.steam, self.steamState = nil, nil
        self:querySteam()
        self:say("Mods rescanned.")
    elseif id == "check" then
        self:request("check", "Check setup asked for.")
    elseif id == "folder" then
        pcall(function() showFolderInDesktop(Core.getMyDocumentFolder() .. getFileSeparator() .. "mods") end)
    end
end

--- Leave for another screen (fn runs after this one closes): unaccepted changes get the Back question first.
function PZMMMods:leaveTo(fn)
    if self:changes(true) == 0 then
        self:close()
        return fn()
    end
    self:ask({ title = "Accept your changes to your mods before you leave?", choices = {
        { "Accept", function()
            if self.defChanged then return self:onAccept() end -- (asks its own question; leave again after)
            self:acceptNow()
            fn()
        end },
        { "Discard", function() self:onBackAnswer({ internal = "NO" }); fn() end },
    } })
end
function PZMMMods:onVanilla()
    self:leaveTo(function() menuClick({ internal = "MODS" }, 0, 0) end)
end

--- Steam's Internet server list. The game's own server browser listens to the same event and, until it
--- has been opened, its counters are nil: it errors on every server that answers unless they start at 0.
function PZMMMods.askInet()
    if MultiplayerUI then
        MultiplayerUI.received = MultiplayerUI.received or 0
        MultiplayerUI.serverCount = MultiplayerUI.serverCount or 0
    end
    pcall(function() steamRequestInternetServersList() end)
end

function PZMMMods.open()
    local s = PZMMMods.instance
    local w, h = getCore():getScreenWidth(), getCore():getScreenHeight()
    local large = SETTINGS.large == true
    local compact = SETTINGS.compact == true
    if not s or s.width ~= w or s.height ~= h or s.large ~= large or s.compact ~= compact then
        if s then s:removeFromUIManager() end
        measure(large, compact)
        s = PZMMMods:new(0, 0, w, h)
        s.large = large
        s.compact = compact
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
    s.pending = s.pending or {}
    s.afterAck = s.afterAck or {}
    s.sent = s.sent or {}
    s.hidden, s.picked, s.anchor, s.neededAll = readHidden(), {}, nil, nil
    s.ignored, s.langDone, s.lang, s.homeAsked, s.lint, s.srvSel = nil, false, nil, nil, nil, nil
    s.hosted, s.recent, s.srvCache, s.pfAt, PZMMMods.pitchAt = nil, nil, nil, nil, nil
    s.looked, s.lookOrder = nil, nil
    s.undos, s.redos, s.odrag, s.cats, s.diffAt, s.mapUndos = {}, {}, nil, nil, nil, nil
    s.lib, s.app, s.feed = s:readLib(), readApp(), readFeed()
    s.tags = readTags()
    s.installed, s.hasInstalled = readInstalled()
    s.firstSeen = K.readFirstSeen()
    pcall(function() s:sendLeaveOuts() end)
    s.bisect = readBisect()
    applyTheme(s.app)
    s.defRev, s.defChanged, s.openedAt, s.appAt = s.app and s.app.default_rev, nil, getTimestampMs(), getTimestampMs()
    s.mapOrder, s.modsOrig, s.saves, s.java = nil, nil, nil, nil
    s.mapScanState, s.mapShow, s.mapSel, s.gridCache, s.mapRowsKey, s.legendKey, s.gv = nil, {}, nil, nil, nil, nil, nil
    s.mapClash, s.gridSel, s.cellVer, s.mapRowsList, s.legendList = nil, nil, (s.cellVer or 0) + 1, nil, nil
    s.steam, s.steamState, s.liteFeedCache, s.lastPlayed, s.feedSeen = nil, nil, nil, readPlayed(), readFeedSeen()
    s:querySteam()
    s.page, s.feedScroll, s.launchAt, s.latest = s.page or "mods", 0, nil, nil
    -- a search for the broken mod that's still going (or has an answer): its panel is the first thing you see
    if s.bisect then s.page, s.checkOpen, s.selected = "mods", true, nil end
    s:loadRuleFile()
    s.model:reloadMods()
    local keep = PZMMMods.carry
    PZMMMods.carry = nil
    -- the bridge's state survives the rebuild: what was asked of PZMM and not answered yet
    if keep and keep.bridge then
        local b = keep.bridge
        s.pending, s.sent, s.sentAt, s.kinds, s.afterAck, s.overlay = b.pending or {}, b.sent or {}, b.sentAt, b.kinds, b.afterAck or {}, b.overlay
        s.defRev, s.defChanged = b.defRev, b.defChanged
    end
    if keep and keep.cur then
        -- the Larger text / Compact rebuild: what wasn't accepted yet stays so (the old baseline, list and favourites)
        pcall(function() s.model:getActiveMods():copyFrom(keep.cur) end)
        for id, v in pairs(keep.favs) do s.model.favs[id] = v end
        s.model:refreshMods()
        for id, d in pairs(s.model.mods) do
            local k = keep.defs[id]
            if k then d.defaultActive, d.defaultFav = k[1], k[2] end
        end
        s.modsOrig = keep.orig
    end
    if not s.modsOrig then
        pcall(function()
            local o = ActiveMods.getById("pzmmUndo")
            o:copyFrom(s.model:getActiveMods())
            s.modsOrig = o
        end)
    end
end

-- when you last started a game (Zomboid/Lua/pzmm/played.txt), for What's new without the app
local function savePlayed()
    pcall(function()
        local w = getFileWriter("pzmm/played.txt", true, false)
        w:write(tostring(now()))
        w:close()
    end)
end
if Events.OnGameStart then Events.OnGameStart.Add(savePlayed) end

--- The next mod in the list after (or before) the picked one, kept in view.
function PZMMMods:stepSelection(dir)
    local mods, at = {}, nil
    for _, e in ipairs(self.entries or {}) do
        if e.mod then
            mods[#mods + 1] = e.mod
            if e.mod == self.selected then at = #mods end
        end
    end
    if #mods == 0 then return end
    at = math.max(1, math.min(#mods, (at or (dir > 0 and 0 or #mods + 1)) + dir))
    self.selected, self.checkOpen, self.picked = mods[at], false, {}
    -- scroll so it shows
    local _, ly, _, lh = self:listBox()
    local y = 0
    for i, e in ipairs(self.entries) do
        if e.mod == self.selected then break end
        y = y + (e.head and (HEAD + (i > 1 and GAP or 0)) or ROW)
    end
    if y < self.scroll then self.scroll = y elseif y + ROW > self.scroll + lh then self.scroll = y + ROW - lh end
end

--- Alt+Up / Alt+Down: the picked mod one row up or down, the same as dragging it there.
function PZMMMods:nudge(dir)
    local d = self.selected
    if self:filtering() or not d or d.ghost then return end
    local es, at = self.entries or {}, nil
    for i, e in ipairs(es) do if e.mod == d then at = i end end
    local n = at and es[at + dir]
    if not n then return end
    local g, before
    if dir < 0 then
        if n.mod then
            g, before = n.g, n.mod.modId
        else
            -- past its section's top: the end of the section above
            local p = es[at - 2]
            if not p then return end
            g = p.head or p.g
        end
    else
        if n.mod then
            g, before = n.g, n.nextId
        else
            g, before = n.head, n.first
        end
    end
    if not g or not self:ensureLibrary() then return end
    if not (g == "Other" or self.lib.header[g]) then return end
    self:placeMod(d, g, before)
end

function PZMMMods.onKey(key)
    local s = PZMMMods.instance
    if not s or not s:isVisible() then return end
    local ov = PZMMOverlay.current
    if key == Keyboard.KEY_ESCAPE then
        if ov then return ov:closeIt() end
        -- a box being typed in lets go first
        local typing0 = false
        pcall(function() typing0 = (s.search:isFocused() or s.homeSearch:isFocused()) and true or false end)
        if typing0 then
            pcall(function() s.search:unfocus() end)
            pcall(function() s.homeSearch:unfocus() end)
            return
        end
        if s.drag or s.odrag then s.drag, s.odrag = nil, nil return end
        -- then the picked mods, then the selected one
        for _ in pairs(s.picked or {}) do s.picked = {} return end
        if s.checkOpen then s.checkOpen = false return end
        if s.selected then s.selected = nil return end
        return s:onBack()
    end
    if ov then return ov:onKey(key) end
    if PZMMMods.overlayShut and getTimestampMs() - PZMMMods.overlayShut < 150 then return end
    local typing = false
    pcall(function() typing = s.search:isFocused() end)
    if typing then return end
    local ctrl = false
    pcall(function() ctrl = isCtrlKeyDown() end)
    -- the list they undo is on the My mods page only
    local shift = false
    pcall(function() shift = isShiftKeyDown() end)
    if ctrl and s.page == "mods" and key == Keyboard.KEY_Z then return s:undoRedo(not shift) end
    if ctrl and s.page == "mods" and key == Keyboard.KEY_Y then return s:undoRedo(false) end
    if Keyboard.KEY_SLASH and key == Keyboard.KEY_SLASH and s.page == "mods" and not ctrl then
        s.slashAt = getTimestampMs()
        pcall(function() s.search:focus() end)
        return
    end
    if ctrl and key == Keyboard.KEY_F then
        s.page = "mods"
        pcall(function() s.search:focus() end)
        return
    end
    if key == Keyboard.KEY_PRIOR or key == Keyboard.KEY_NEXT then
        local tabs, at = {}, 1
        for _, pg in ipairs(PAGES) do
            if not pg.hidden then
                tabs[#tabs + 1] = pg[1]
                if pg[1] == s.page then at = #tabs end
            end
        end
        at = (at + (key == Keyboard.KEY_NEXT and 1 or -1) - 1) % #tabs + 1
        s.page, s.feedScroll = tabs[at], 0
        return
    end
    if s.page ~= "mods" then return end
    local alt = false
    pcall(function() alt = Keyboard.isKeyDown(Keyboard.KEY_LMENU) or Keyboard.isKeyDown(Keyboard.KEY_RMENU) end)
    if ctrl and key == Keyboard.KEY_A then
        -- every mod the list shows
        s.picked = {}
        for _, e in ipairs(s.entries or {}) do if e.mod then s.picked[e.mod.modId] = true end end
        return
    end
    if alt and s.selected and (key == Keyboard.KEY_UP or key == Keyboard.KEY_DOWN) then return s:nudge(key == Keyboard.KEY_UP and -1 or 1) end
    if key == Keyboard.KEY_UP then return s:stepSelection(-1) end
    if key == Keyboard.KEY_DOWN then return s:stepSelection(1) end
    if key == Keyboard.KEY_RETURN and not s.selected and s.orderSel and s.model.mods[s.orderSel] then
        s.selected, s.checkOpen = s.model.mods[s.orderSel], false
        return
    end
    if key == Keyboard.KEY_RETURN and not s.selected then return s:stepSelection(1) end -- opens the first mod's details
    if key == Keyboard.KEY_SPACE and s.selected then return s:toggle(s.selected) end
end
if Events.OnKeyPressed then Events.OnKeyPressed.Add(PZMMMods.onKey) end

-- the main menu's Mods button opens this screen
MainScreen.onMenuItemMouseDownMainMenu = function(item, x, y)
    if item.internal == "MODS" then
        -- ponytail: no controller support here yet, so a controller gets the game's own mod screen
        local pad = nil
        pcall(function() pad = JoypadState.getMainMenuJoypad() end)
        if pad then return menuClick(item, x, y) end
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
                -- a plain bullet reads as plain text (no bold lead)
                notes[#notes + 1] = { "", (line:gsub("^%-%s+", "")) }
            end
            line = r:readLine()
        end
        r:close()
    end)
    return notes
end
K.NOTES = {}
for _, n in ipairs({ readNotes("game", "In-game mod manager", "notes_game.txt"), readNotes("app", "Desktop app", "notes_app.txt") }) do
    if n.version ~= "" then K.NOTES[#K.NOTES + 1] = n end
end
K.RELEASES = "https://github.com/SEVNC3/pz-mod-manager/releases/latest"

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
K.NEWS_W, K.NEWS_OPEN_W, K.NEWS_HEAD, K.NEWS_TABS = 280, 460, 60, H_SMALL + 19
K.NEWS_GAP = 12 -- between the versions line and the tabs, open
-- PZMM - Client on the Workshop: where a thumbs-up helps
K.STEAM_PAGE = "https://steamcommunity.com/sharedfiles/filedetails/?id=3818460225"
K.WHITE, K.PROMO_BG, K.PROMO_LINE, K.PROMO_RED, K.PROMO_TEXT = hex("ffffff"), hex("141414"), hex("2e2e2e"), hex("e0382f"), hex("e8e8e6")
--- The promo banner's height: a strip of tape, a line under it.
function K.promoH() return 12 + math.max(24, H_MARKER + 2) + 6 + H_SMALL + 12 end
K.BLANK = tex("blank")

function PZMMNews:new()
    local o = ISPanel:new(0, 0, K.NEWS_W, K.NEWS_HEAD)
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
    self.bars = {}
    local main = MainScreen.instance
    -- the pause menu is the main screen too: off screen there
    if not main or main.inGame then
        self:setX(-10000)
        return
    end
    if getTimestampMs() - settingsAt > 3000 then loadSettings() end
    -- the main menu without the game's logo and ZombieBuddy's "loaded" watermark over the top of the screen
    main.pzmmLogo = main.pzmmLogo or main.logoTexture
    main.logoTexture = (SETTINGS.logo and K.BLANK) or main.pzmmLogo
    if ZombieBuddy and ZombieBuddy.Watermark and ZombieBuddy.Watermark.setAlpha then
        local a = 0
        if not SETTINGS.watermark then
            a = 0.4
            local mo = PZAPI and PZAPI.ModOptions
            local opts = mo and mo.getOptions and mo:getOptions("ZombieBuddy")
            local o = opts and opts.getOption and opts:getOption("watermarkOpacity")
            if o and o.getValue then a = o:getValue() end
        end
        pcall(function() ZombieBuddy.Watermark.setAlpha(a) end)
    end
    if not SETTINGS.news then
        self:setX(-10000)
        return
    end

    local w = self.open and K.NEWS_OPEN_W or K.NEWS_W
    self:setWidth(w)
    self:setX(main.width - w - 24)
    self:setY(24)
    local mx, my = self:getMouseX(), self:getMouseY()
    local lines
    local h = K.NEWS_HEAD
    if self.open then
        local c = self.cache
        if not (c and c.tab == self.tab and c.w == w) then
            c = { tab = self.tab, w = w, lines = {} }
            for _, note in ipairs(self.tab) do
                if note[1] ~= "" then for _, l in ipairs(wrap(note[1], w - 48)) do c.lines[#c.lines + 1] = { l, true } end end
                for _, l in ipairs(wrap(note[2], w - 48)) do c.lines[#c.lines + 1] = { l, false } end
                c.lines[#c.lines + 1] = { "", false }
            end
            self.cache = c
        end
        lines = c.lines
        -- the notes scroll inside a capped body, so a long patch never runs off the screen
        self.bodyFull = #lines * (H_SMALL + 2)
        local room = getCore():getScreenHeight() - 24 - 120 - (K.NEWS_HEAD + K.NEWS_GAP + K.NEWS_TABS + 14 + H_SMALL + 28)
        self.bodyH = math.min(self.bodyFull, math.max(120, math.min(420, room)))
        self.newsScroll = math.max(0, math.min(self.newsScroll or 0, self.bodyFull - self.bodyH))
        h = h + K.NEWS_GAP + K.NEWS_TABS + 14 + self.bodyH + H_SMALL + 28 + K.promoH() + 12
    end
    self:setHeight(h)
    round(self, 0, 0, w, h, 10, C.panel, 0.94)
    outline(self, 0, 0, w, h, 10, C.line, 1)
    if my >= 0 and my < K.NEWS_HEAD and mx >= 0 and mx < w then
        round(self, 1, 1, w - 2, K.NEWS_HEAD - 2, 10, C.hover, 0.6)
        tip(self, self.open and "Hide the patch notes" or "PZMM patch notes\nWhat changed in the in-game mod manager and the desktop app. The red ! marks a patch you haven't read.")
    end
    image(self, K.LOGO, 10, 10, 40, 40)
    bold(self, "PZMM patch notes", 60, 10, C.text, MEDIUM)
    -- each part's name and version, in pixel gaps (this font draws a space almost no width)
    local fresh, vx = false, 60
    for _, n in ipairs(K.NOTES) do
        local name = n.kind == "game" and "In game" or "App"
        text(self, name, vx, 12 + H_MEDIUM, C.muted)
        vx = vx + width(name) + 6
        text(self, n.version, vx, 12 + H_MEDIUM, C.text)
        vx = vx + width(n.version) + 20
        fresh = fresh or self:fresh(n)
    end
    local rx = w - 14 - width("+", MEDIUM)
    text(self, self.open and "-" or "+", rx, (K.NEWS_HEAD - H_MEDIUM) / 2, C.muted, MEDIUM)
    if fresh then bang(self, rx - 32, math.floor((K.NEWS_HEAD - 20) / 2)) end
    if not self.open then return end

    -- the tabs, as the app's: a red wash and a white underline on the open or hovered one
    local tx, top = 12, K.NEWS_HEAD + K.NEWS_GAP
    self.tabBoxes = {}
    for _, n in ipairs(K.NOTES) do
        local label = n.label
        local mark = self:fresh(n)
        local tw = width(label) + 10 + width(n.version) + 28 + (mark and 28 or 0)
        local on = self.tab == n
        local over = mx >= tx and mx < tx + tw and my >= top and my < top + K.NEWS_TABS
        if over then
            tip(self, (n.kind == "game" and "In-game mod manager\nThis screen, PZMM - Client, in the game." or "Desktop app\nThe PZMM app for Windows and Linux.")
                .. (mark and "\nNew: you haven't read this patch yet." or ""))
        end
        if on or over then
            round(self, tx, top, tw, K.NEWS_TABS - 1, 3, C.accent, 0.32, true, false)
            rect(self, tx + 3, top, tw - 6, 1, C.accent, 0.7)
            rect(self, tx, top + 3, 1, K.NEWS_TABS - 4, C.accent, 0.7)
            rect(self, tx + tw - 1, top + 3, 1, K.NEWS_TABS - 4, C.accent, 0.7)
            rect(self, tx, top + K.NEWS_TABS - 3, tw, 2, C.text, 1)
        end
        local ly = top + (K.NEWS_TABS - H_SMALL) / 2
        text(self, label, tx + 14, ly, (on or over) and C.onSelected or C.muted)
        bold(self, n.version, tx + 14 + width(label) + 10, ly, (on or over) and C.onSelected or C.text)
        if mark then bang(self, tx + 14 + width(label) + 10 + width(n.version) + 8, top + math.floor((K.NEWS_TABS - 20) / 2)) end
        self.tabBoxes[#self.tabBoxes + 1] = { n, tx, tw }
        tx = tx + tw + 2
    end
    rect(self, 0, top + K.NEWS_TABS - 1, w, 1, C.line, 1)

    local by = top + K.NEWS_TABS + 14
    self:setStencilRect(0, top + K.NEWS_TABS, w, self.bodyH + 14)
    local y = by - self.newsScroll
    for _, l in ipairs(lines) do
        if y + H_SMALL > top + K.NEWS_TABS and y < by + self.bodyH then
            if l[2] then bold(self, l[1], 24, y, C.text) else text(self, l[1], 24, y, C.muted) end
        end
        y = y + H_SMALL + 2
    end
    self:clearStencilRect()
    if self.bodyFull > self.bodyH then
        -- the scroll bar (What's new's), and a line where the notes are cut off
        scrollbar(self, "newsScroll", 0, by, w, self.bodyH, self.bodyFull)
        rect(self, 12, by + self.bodyH + 3, w - 24, 1, C.line, 1)
    end
    y = by + self.bodyH + 10
    -- the app's promo: a dark banner with its grid, a strip of tape, marker writing; all of it opens the Steam page
    local ph, px0, pw0 = K.promoH(), 12, w - 24
    local pover = mx >= px0 and mx < px0 + pw0 and my >= y and my < y + ph
    round(self, px0, y, pw0, ph, 6, K.PROMO_BG, 1)
    self:setStencilRect(px0, y, pw0, ph)
    for gx = px0, px0 + pw0, 24 do rect(self, gx, y, 1, ph, K.WHITE, 0.06) end
    for gy = y, y + ph, 24 do rect(self, px0, gy, pw0, 1, K.WHITE, 0.06) end
    self:clearStencilRect()
    outline(self, px0, y, pw0, ph, 6, pover and C.accent or K.PROMO_LINE, pover and 0.9 or 1)
    local tapeH = math.max(24, H_MARKER + 2)
    tape(self, T("promo_enjoying", "Enjoying PZMM?"), px0 + 10, y + 6 + tapeH / 2)
    local cm = T("promo_click", "click me")
    text(self, cm, px0 + pw0 - 14 - width(cm, MARKER), y + 6 + (tapeH - H_MARKER) / 2, K.PROMO_RED, MARKER)
    text(self, T("promo_ask", "Give it a thumbs-up on the Workshop"), px0 + 14, y + 6 + tapeH + 6, K.PROMO_TEXT)
    if pover then tip(self, "Upvote on the Workshop\nPZMM - Client's page in Steam.") end
    self.promoY = y
    y = y + ph + 12
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

function PZMMNews:onMouseUp(x, y) self.sbDrag = nil return true end
function PZMMNews:onMouseUpOutside(x, y) self.sbDrag = nil return true end

function PZMMNews:onMouseDown(x, y)
    if barDown(self, x, y) then return true end
    if y < K.NEWS_HEAD then
        getSoundManager():playUISound("UISelectListItem")
        self.open = not self.open
        if self.open then
            -- the first tab with something new, else the one last open
            local pick = self.tab or K.NOTES[1]
            for _, n in ipairs(K.NOTES) do
                if self:fresh(n) then pick = n break end
            end
            self:show(pick)
        end
        return true
    end
    if not self.open then return true end
    if y < K.NEWS_HEAD + K.NEWS_GAP + K.NEWS_TABS then
        if y < K.NEWS_HEAD + K.NEWS_GAP then return true end
        for _, t in ipairs(self.tabBoxes or {}) do
            if x >= t[2] and x < t[2] + t[3] then
                getSoundManager():playUISound("UISelectListItem")
                self:show(t[1])
            end
        end
    elseif self.promoY and y >= self.promoY and y < self.promoY + K.promoH() and x >= 12 and x < self.width - 12 then
        openLink(K.STEAM_PAGE)
    elseif self.linkY and y >= self.linkY and y < self.linkY + H_SMALL + 4 and x >= 24 and x < 24 + width("Full notes on GitHub") then
        openLink(K.RELEASES)
    end
    return true
end

-- the app's community note (Home.svelte), drawn into scrap_note.png by .temp/noteart.py: { label, url, its
-- box on the paper before the tilt }. scrap_note_<i>.png has link i in blood, underlined (hovered).
K.COMMUNITY = {
    { "Project Zomboid website", "https://projectzomboid.com/blog/", { 30, 68, 276, 21 } },
    { "The Indie Stone forums", "https://theindiestone.com/forums/", { 30, 89, 276, 21 } },
    { "Project Zomboid Discord", "https://discord.gg/theindiestone", { 30, 110, 276, 21 } },
    { "PZwiki", "https://pzwiki.net/wiki/Project_Zomboid_Wiki", { 30, 131, 276, 21 } },
    { "PZwiki: Modding", "https://pzwiki.net/wiki/Modding", { 30, 152, 276, 21 } },
    { "PZ Modding Community Discord (unofficial)", "https://discord.gg/ghRTHUTc94", { 30, 173, 276, 38 } },
    { "r/projectzomboid on Reddit", "https://www.reddit.com/r/projectzomboid/", { 30, 211, 276, 21 } },
    { "Steam Workshop", "https://steamcommunity.com/app/108600/workshop/", { 30, 232, 276, 21 } },
    { "PZMM Discord", "https://discord.com/invite/6zKZuZ5WWe", { 30, 253, 276, 21 } },
}
K.NOTE_W, K.NOTE_H, K.NOTE_TILT = 332, 340, math.rad(-2) -- the paper before the tilt, and the tilt
K.NOTE_HOVER = {}
for i = 1, #K.COMMUNITY do K.NOTE_HOVER[i] = tex("scrap_note_" .. i) end
-- the hero per profile, as the app's: Project Zomboid's Steam art and logo, or Project Viewpoint's
K.HERO_PZ, K.HERO_VP, K.LOGO_PZ, K.VIEWPOINT_LOGO = tex("hero_pz"), tex("hero_viewpoint"), tex("logo_pz"), tex("viewpoint_logo")
K.NOTE = tex("scrap_note")

-- --- the app's other tabs, as far as the game lets a mod go: each draws into the What's new frame (feedBox,
-- feedScroll, feedHits) and returns the y under it. Tabs the game can't do get the We're sorry card; tabs it
-- can do in part start with a notice of what's only in the app.

--- The game opens only https pages, so PZMM starts through this page: it opens pzmm://game.
K.LAUNCH = "https://sevnc3.github.io/pz-mod-manager/open/"

--- Ask the PZMM app to do something (Zomboid/Lua/pzmm/requests.txt, a line per request: `<at>-<n>`, the
--- profile shown, then install <ids>, backup <mode/name>, check, open <tab>, place..., section...). A running
--- PZMM takes them within a second and comes to the front (it drops those over a minute old); if they're still
--- there a few seconds on and it isn't running, the launch page starts it. It asks before it installs or
--- backs anything up, and acks quiet ones (acks.txt), which settles the edits shown here.
function PZMMMods:request(line, said, quiet)
    self:appUp()
    PZMMMods.seq = (PZMMMods.seq or 0) + 1
    local id = tostring(now()) .. "-" .. tostring(PZMMMods.seq)
    local profile = ((self.lib and self.lib.profile) or ""):gsub("[\t\r\n]", " ")
    local ok = pcall(function()
        local w = getFileWriter("pzmm/requests.txt", true, true)
        w:write(id .. "\t" .. profile .. "\t" .. line .. "\n")
        w:close()
    end)
    if ok and quiet then self.pending[id] = getTimestampMs() end
    if ok and not quiet then
        self.sent[id] = said
        self.kinds = self.kinds or {}
        self.kinds[id] = line:match("^%a+")
        self.sentAt = self.sentAt or {}
        self.sentAt[id] = getTimestampMs()
    end
    if ok and not quiet then self.launchAt = getTimestampMs() + 2500 end
    -- (a quiet request may say nothing: say() skips an empty line)
    self:say(ok and ((said or "") .. (quiet and "" or " Opening PZMM...")) or "Couldn't reach PZMM: Zomboid/Lua/pzmm/requests.txt can't be written.")
    return ok and id or nil
end

--- PZMM didn't take the requests and isn't running, so start it.
function PZMMMods:launchIfNeeded()
    if not (self.launchAt and getTimestampMs() >= self.launchAt) then return end
    self.launchAt = nil
    if self:appUp() then return end
    self:launchNow()
end

--- Start PZMM through the launch page when a request is still waiting in requests.txt.
function PZMMMods:launchNow()
    local waiting = false
    pcall(function()
        local r = getFileReader("pzmm/requests.txt", false)
        if r then waiting = true; r:close() end
    end)
    if waiting then
        openLink(K.LAUNCH)
        self:say("Starting PZMM: if your browser asks, let it open PZMM.")
    end
end

--- The main menu's own screens: this one closes, then the game opens its own (Load, Multiplayer, Workshop...).
function PZMMMods:gameScreen(internal)
    self:leaveTo(function() menuClick({ internal = internal }, 0, 0) end)
end

--- The notice at the top of a tab the game can do in part: what's only in the app, and where to get it.
--- A banner as the app's: a card (16 px side margins, radius 8, shadow, no bottom line), an icon, one sentence and
--- plain buttons with icons. b = { text, tone ("accent": the latest-save tint; else danger), buttons = { { label, icon, tip, fn | url } } }.
--- Returns its height, its lines and its buttons' total width.
function K.bannerH(ui, b, w)
    local bw = 0
    for i, x in ipairs(b.buttons) do bw = bw + width(x.label) + (x.icon and 20 or 0) + 28 + (i > 1 and 12 or 0) end
    local room = (w or ui.width - 32) - 44 - 16 - (bw > 0 and bw + 12 or 0)
    local lines = wrap(b.text, room)
    return math.max(BAR + 16, #lines * (H_SMALL + 2) + 12), lines, bw
end
--- Draw it at `y` (the card's top); its buttons go in `hits`. Returns its height.
function K.drawBanner(ui, y, b, mx, my, hits, x, w)
    x, w = x or 16, w or ui.width - 32
    local h, lines, bw = K.bannerH(ui, b, w)
    card(ui, x, y, w, h)
    round(ui, x, y, w, h, 8, b.tone == "accent" and C.accent or C.danger, 0.12)
    if TB.alert then ui:drawTextureScaled(TB.alert, x + 16, y + math.floor((h - 16) / 2), 16, 16, 1, C.danger.r, C.danger.g, C.danger.b) end
    local ty = y + math.floor((h - (#lines * (H_SMALL + 2) - 2)) / 2)
    for _, l in ipairs(lines) do
        text(ui, l, x + 44, ty, C.text)
        ty = ty + H_SMALL + 2
    end
    local bx, by = x + w - 16 - bw, y + math.floor((h - BAR) / 2)
    for _, bt in ipairs(b.buttons) do
        local cw = width(bt.label) + (bt.icon and 20 or 0) + 28
        local _, _, over = flatButton(ui, bt.label, bx, by, mx, my, bt.primary and "primary" or nil, bt.icon)
        if over and bt.tip then tip(ui, bt.tip) end
        hits[#hits + 1] = { kind = bt.url and "url" or "fn", url = bt.url, fn = bt.fn, x = bx, y = by, w = cw, h = BAR }
        bx = bx + cw + 12
    end
    return h
end

-- each tab's tab in the app, for Open in PZMM
K.APP_TAB = { home = "home", mods = "mods", feed = "feed", workshop = "workshop", maps = "maps", servers = "servers", saves = "saves", java = "zb", settings = "settings", make = "authoring" }
--- Its height on My mods (under the filters bar), 10 px margin above included, worked out again only when the
--- width or font changes; with the banners under it.
function PZMMMods:modsNoticeH()
    local c = self.noticeCache
    if not c or c.w ~= self.width or c.bar ~= BAR or c.app ~= (self.app ~= nil) then
        c = { w = self.width, bar = BAR, app = self.app ~= nil, h = K.noticeH() + 10 }
        self.noticeCache = c
    end
    return c.h + self:bannersH()
end

--- The banners under My mods' notice: { text, tone, buttons }. Looked at twice a second.
function PZMMMods:banners()
    local t = getTimestampMs()
    local c = self.bannerCache
    if c and t - c.at < 500 then return c.list end
    local list = {}
    -- PZMM changed the mods while this screen was open: take them, or keep this list (Accept writes it)
    if self.defChanged then
        list[#list + 1] = { tone = "accent", text = T("app_changed", "PZMM changed your mods since you opened this screen."),
            buttons = { { label = T("use_app_mods", "Use PZMM's mods"), icon = TB.swap, tip = "Use PZMM's mods\nTurns your mods on and off here as PZMM has them.",
                    fn = function() self:takeAppMods(); self.bannerCache = nil; self:say("Your mods here are PZMM's now.") end },
                { label = T("keep_list", "Keep this list"), tip = "Keep this list\nAccept writes this screen's list over PZMM's.",
                    fn = function() self.defRev, self.defChanged, self.bannerCache = self.app and self.app.default_rev, nil, nil end } } }
    end
    -- the save you'd continue was played with other mods
    if self.latest == nil then self.latest = getLatestSave() or false end
    local latest = self.latest
    if latest and latest[1] and self.saveDismissed ~= latest[1] then
        for _, s in ipairs(self:savesList()) do
            if s.name == latest[1] and (not latest[2] or s.mode == latest[2]) and s.active then
                local diff = self:modsDiffer(s)
                if diff then
                    local on, theirs, more, fewer = {}, {}, 0, 0
                    for _, id in ipairs(self.order or {}) do on[id] = true end
                    for _, id in ipairs(s.mods) do
                        theirs[id] = true
                        if not on[id] then more = more + 1 end
                    end
                    for id in pairs(on) do if not theirs[id] then fewer = fewer + 1 end end
                    local parts = {}
                    if more > 0 then parts[#parts + 1] = more .. (more == 1 and " of its mods isn't enabled" or " of its mods aren't enabled") end
                    if fewer > 0 then parts[#parts + 1] = fewer .. (fewer == 1 and " enabled mod wasn't in it" or " enabled mods weren't in it") end
                    list[#list + 1] = { tone = "accent",
                        text = "Your latest save, " .. s.name .. ", was played with a different mod list: " .. table.concat(parts, " and ") .. ". Continuing it with this list can break it.",
                        buttons = { { label = T("banner_use_mods", "Use this save's mods"), icon = TB.swap, tip = "Use this save's mods\nTurns on exactly this save's mods in My mods. Accept keeps them.",
                                fn = function() self:useSaveMods(s) end },
                            { label = T("dismiss", "Dismiss"), fn = function() self.saveDismissed = s.name; self.bannerCache = nil end } } }
                end
                break
            end
        end
    end
    local zb = self:zbNeeded()
    if zb > 0 then
        list[#list + 1] = { text = zb .. (zb == 1 and " enabled mod needs ZombieBuddy, which isn't set up." or " enabled mods need ZombieBuddy, which isn't set up."),
            buttons = { { label = T("banner_set_up", "Set up ZombieBuddy"), icon = TB.settings, tip = "Set up ZombieBuddy\nJava mods: ZombieBuddy, and the mods that need it.",
                fn = function() self.page, self.feedScroll = "java", 0 end } } }
    end
    self.bannerCache = { at = t, list = list }
    return list
end
--- All banners' heights, their 10 px margins above included.
function PZMMMods:bannersH()
    local h = 0
    for _, b in ipairs(self:banners()) do h = h + (K.bannerH(self, b, self.width - PAD * 2 - SIDE)) + 10 end
    return h
end
--- Draw them from `y`, one card under another; their buttons are kept in bannerHits.
function PZMMMods:drawBanners(y, mx, my)
    self.bannerHits = {}
    for _, b in ipairs(self:banners()) do
        y = y + 10
        y = y + K.drawBanner(self, y, b, mx, my, self.bannerHits, PAD, self.width - PAD * 2 - SIDE)
    end
end

--- The notice's height: the same on every page (two lines, 14 px above and below).
function K.noticeH() return 2 * H_SMALL + 4 + 28 end

--- A tab's missing features as a list: split at its top-level commas and semicolons (not inside brackets).
function K.missingList(missing)
    local out, cur, depth = {}, "", 0
    local function add(item)
        item = item:gsub("^%s+", ""):gsub("%s+$", ""):gsub("^and ", ""):gsub("%.$", "")
        if item ~= "" then out[#out + 1] = item:sub(1, 1):upper() .. item:sub(2) end
    end
    local function push()
        -- an item whose first brackets hold groups split by ";" (Check setup's): its head, a bullet a group,
        -- then what follows the brackets as its own
        local open = cur:find("(", 1, true)
        local close, d = nil, 0
        if open then
            for i = open, #cur do
                local ch = cur:sub(i, i)
                if ch == "(" then d = d + 1 elseif ch == ")" then d = d - 1 end
                if d == 0 then close = i; break end
            end
        end
        local inner = close and cur:sub(open + 1, close - 1)
        if inner and inner:find(";", 1, true) then
            add((cur:sub(1, open - 1):gsub("%s+$", "")) .. ":")
            for part in inner:gmatch("[^;]+") do add(part) end
            add(cur:sub(close + 1))
        else
            add(cur)
        end
        cur = ""
    end
    for ch in tostring(missing):gmatch(".") do
        if ch == "(" then depth = depth + 1 elseif ch == ")" then depth = math.max(0, depth - 1) end
        if depth == 0 and (ch == "," or ch == ";") then push() else cur = cur .. ch end
    end
    push()
    return out
end

--- The notice at the top of a tab the game can do in part: sorry, some features are missing (View lists
--- them), and where the app is. Its buttons and links go in feedHits, or noticeHits (My mods).
function PZMMMods:drawNotice(x, y, w, missing, mx, my, noHits)
    local hits = {}
    if self.page == "mods" then y, x, w = y + 10, PAD, self.width - PAD * 2 - SIDE end
    local h = K.noticeH()
    card(self, x, y, w, h)
    round(self, x, y, w, h, 8, C.danger, 0.12)
    if TB.alert then self:drawTextureScaled(TB.alert, x + 16, y + math.floor((h - 16) / 2), 16, 16, 1, C.danger.r, C.danger.g, C.danger.b) end
    -- the buttons at the right: View, and Open in PZMM when the app is installed
    local page = self.page
    local buttons = { { label = T("view", "View"), icon = TB.eye, tip = "View\nWhat's missing on this tab.", fn = function()
        self:ask({ title = T("missing_modal", "Missing from the in-game manager"), lines = K.missingList(missing), cancel = T("close", "Close"),
            choices = { self.app and { T("open_in_pzmm", "Open in PZMM"), function() self:request("open\t" .. (K.APP_TAB[page] or "home"), "PZMM asked to open.") end, TB.play }
                or { T("get_the_app", "Get the app"), function() openLink(K.RELEASES_URL) end, TB.download } } })
    end } }
    if self.app then
        buttons[2] = { label = T("open_in_pzmm", "Open in PZMM"), icon = TB.play, tip = "Open in PZMM\nThe app, on this tab.",
            fn = function() self:request("open\t" .. (K.APP_TAB[page] or "home"), "PZMM asked to open.") end }
    end
    local bx, by = x + w - 16, y + math.floor((h - BAR) / 2)
    for i = #buttons, 1, -1 do
        local bt = buttons[i]
        local bw = width(bt.label) + (bt.icon and 20 or 0) + 28
        bx = bx - bw
        local _, _, over = flatButton(self, bt.label, bx, by, mx, my, nil, bt.icon)
        if over then tip(self, bt.tip) end
        hits[#hits + 1] = { kind = "fn", fn = bt.fn, x = bx, y = by, w = bw, h = BAR }
        bx = bx - 8
    end
    -- the words: sorry, then where to get the app, its GitHub and Discord as links
    local tx = x + 44
    local room = bx - 16 - tx
    local ty = y + math.floor((h - (2 * H_SMALL + 4)) / 2)
    bold(self, fit(T("sorry_title", "We're sorry! Because of limits on what mods can do in the game, some features are missing here."), room), tx, ty, C.text)
    ty = ty + H_SMALL + 4
    -- with the app installed it's there already: Open in PZMM, not where to get it
    local lead = self.app and T("sorry_open", "They're in the PZMM app: Open in PZMM takes you there.") or (T("sorry_get", "To use them, install the desktop app:") .. " ")
    text(self, fit(lead, room), tx, ty, C.muted)
    local lx = tx + width(lead) + 4 -- (a trailing space measures nothing)
    for i, l in ipairs(self.app and {} or { { "GitHub", K.RELEASES_URL }, { "Discord", LINKS[2][2] } }) do
        local lw = width(l[1])
        if lx + lw > tx + room then break end
        local over = mx >= lx and mx < lx + lw and my >= ty and my < ty + H_SMALL
        text(self, l[1], lx, ty, over and C.text or C.accentText)
        rect(self, lx, ty + H_SMALL - 2, lw, 1, over and C.text or C.accentText, 1)
        hits[#hits + 1] = { kind = "url", url = l[2], x = lx, y = ty, w = lw, h = H_SMALL }
        lx = lx + lw
        if i == 1 then
            text(self, "/", lx + 8, ty, C.muted) -- (the game's font has no middle dot)
            lx = lx + 8 + width("/") + 8
        end
    end
    if not noHits then for _, h2 in ipairs(hits) do self.feedHits[#self.feedHits + 1] = h2 end end
    self.noticeHits = hits
    return y + h + 12
end

--- A row inside a list card: a full-width line under all but the last; hovered, an accent pill with the
--- selection mark at its left; `sel` (the picked row): accent fill, a 2 px white edge and the mark.
--- Returns whether it's hovered.
local function listRow(ui, x, y, w, h, last, mx, my, sel, flat)
    if not last then rect(ui, x, y + h - 1, w, 1, C.line, 0.6) end
    local over = mx >= x and mx < x + w and my >= y and my < y + h
    if sel then
        round(ui, x + 1, y + 1, w - 2, h - 2, 4, C.accent, 0.32)
        outline(ui, x + 1, y + 1, w - 2, h - 2, 4, C.accent, 0.7)
        if K.SEL_MARK then ui:drawTextureScaled(K.SEL_MARK, x - 5, y + math.floor(h / 2) - 7, 10, 14, 1, 1, 1, 1) end
    elseif over and flat then
        -- a drag list's rows are square and softer: they aren't picked, only moved
        rect(ui, x, y, w, h, C.accent, 0.24)
        -- the app's inset 1 px hover-line
        rect(ui, x, y, w, 1, C.accent, 0.7); rect(ui, x, y + h - 1, w, 1, C.accent, 0.7)
        rect(ui, x, y + 1, 1, h - 2, C.accent, 0.7); rect(ui, x + w - 1, y + 1, 1, h - 2, C.accent, 0.7)
        if K.SEL_MARK then ui:drawTextureScaled(K.SEL_MARK, x - 5, y + math.floor(h / 2) - 7, 10, 14, 1, 1, 1, 1) end
    elseif over then
        round(ui, x + 1, y + 1, w - 2, h - 2, 4, C.accent, 0.34)
        outline(ui, x + 1, y + 1, w - 2, h - 2, 4, C.accent, 0.7)
        if K.SEL_MARK then ui:drawTextureScaled(K.SEL_MARK, x - 5, y + math.floor(h / 2) - 7, 10, 14, 1, 1, 1, 1) end
    end
    return over
end

-- Home -------------------------------------------------------------------------------------------------

-- what Steam answers about a saved server (players, ping), by "host:port", for Home's server cards
PZMMMods.srvInfo = PZMMMods.srvInfo or {}
PZMMMods.srvFail = PZMMMods.srvFail or {}
local function onResponded2(host, port, sv2)
    pcall(function()
        PZMMMods.srvFail[tostring(host) .. ":" .. tostring(port)] = nil
        local i = { players = tonumber(sv2:getPlayers()) or 0, max = tonumber(sv2:getMaxPlayers()) or 0, ping = tonumber(sv2:getPing()) or 0, at = getTimestampMs() }
        pcall(function()
            i.ver = tostring(sv2:getVersion() or "")
            i.mapFull = tostring(sv2:getMapName() or "")
            i.map = i.mapFull:match("^[^;]+") or ""
            i.pw = sv2:isPasswordProtected() and true or false
        end)
        PZMMMods.srvInfo[tostring(host) .. ":" .. tostring(port)] = i
        PZMMMods.srvN = (PZMMMods.srvN or 0) + 1
    end)
end
-- a host name's address, looked up once (the lookup can block): ip, else what the game resolves it to
K.HOSTS = {}
local function hostOf(ip)
    if K.HOSTS[ip] == nil then
        K.HOSTS[ip] = ip
        pcall(function() K.HOSTS[ip] = getHostByName(ip) or ip end)
    end
    return K.HOSTS[ip]
end
-- Steam says a server didn't answer: forget what it said and mark it not answering
local function onFailed2(host, port)
    local key = tostring(host) .. ":" .. tostring(port)
    PZMMMods.srvInfo[key] = nil
    PZMMMods.srvFail[key] = true
end
local function listenResponded2()
    if not PZMMMods.listening2 and Events.OnSteamServerResponded2 then
        Events.OnSteamServerResponded2.Add(onResponded2)
        PZMMMods.listening2 = true
    end
    if not PZMMMods.listeningFail and Events.OnSteamServerFailedToRespond2 then
        Events.OnSteamServerFailedToRespond2.Add(onFailed2)
        PZMMMods.listeningFail = true
    end
end

--- A time as "3 h ago", "2 days ago".
function K.ago(t)
    local d = now() - (tonumber(t) or 0)
    if d < 3600 then return math.max(1, math.floor(d / 60)) .. " min ago" end
    if d < 86400 then return math.floor(d / 3600) .. " h ago" end
    local days = math.floor(d / 86400)
    return days == 1 and "yesterday" or (days .. " days ago")
end

--- The links in a server's description: its Discord and its website (the first other link).
function K.descLinks(desc)
    local clean = tostring(desc or ""):gsub("<[^>]*>", " ")
    local discord, site
    for url in clean:gmatch("https?://[%w%-%._~:/%?#@!%$&'%*%+,;=%%]+") do
        local u = url:gsub("[%.,;]+$", "")
        if not discord and (u:find("discord.gg", 1, true) or u:find("discord.com", 1, true)) then
            discord = u
        elseif not site and not u:find("discord", 1, true) then
            site = u
        end
    end
    if not discord then
        local code = clean:match("discord%.gg/([%w%-]+)")
        if code then discord = "https://discord.gg/" .. code end
    end
    return discord, site
end

K.LIB_TABS = { { "popular", T("home_popular", "Popular") }, { "favourites", T("home_favourites", "Favourites") }, { "recent", T("home_recent", "Recent") }, { "hosted", T("home_hosted", "Your servers") } }

--- What the library lists on a tab: { name, ip, port, host, key, sv, recent, hosted, inet }. Popular is the busiest of the
--- Internet list (once Steam has answered); Favourites the game's saved servers; Recent the ones you joined, as PZMM
--- noted them; Your servers the ones you host. Kept until the page is left (and the Internet list grows).
function PZMMMods:libraryList(tab)
    self.libLists = self.libLists or {}
    local c = self.libLists[tab]
    local n = tab == "popular" and #PZMMMods.inet or 0
    if c and c.n == n then return c.list end
    local list = {}
    if tab == "favourites" then
        local favs = {}
        pcall(function() favs = getServerList() or {} end)
        for _, sv in ipairs(favs) do
            local e = { sv = sv, name = "", ip = "", port = 16261 }
            pcall(function() e.name, e.ip, e.port = sv:getName() or "", sv:getIp() or "", sv:getPort() or 16261 end)
            e.host = hostOf(e.ip)
            e.key = e.ip .. ":" .. tostring(e.port)
            list[#list + 1] = e
        end
    elseif tab == "recent" then
        for _, r in ipairs(self:recentServers()) do
            local ip, port = r.addr:match("^(.*):(%d+)$")
            if not ip then ip, port = r.addr, "16261" end
            list[#list + 1] = { name = r.name, ip = ip, port = port, host = hostOf(ip), recent = r, key = ip .. ":" .. port }
        end
    elseif tab == "hosted" then
        for _, st in ipairs(self:hostedList()) do
            local name = ""
            pcall(function() name = st:getName() end)
            list[#list + 1] = { name = name, hosted = st, key = "hosted:" .. name }
        end
    else
        local rows = {}
        for _, sv in ipairs(PZMMMods.inet) do
            local e = { sv = sv, inet = true, name = "", ip = "", port = 16261, pl = 0 }
            pcall(function() e.name, e.ip, e.port, e.pl = sv:getName() or "", sv:getIp() or "", sv:getPort() or 16261, tonumber(sv:getPlayers()) or 0 end)
            e.host = e.ip
            e.key = e.ip .. ":" .. tostring(e.port)
            rows[#rows + 1] = e
        end
        table.sort(rows, function(a, b)
            if a.pl ~= b.pl then return a.pl > b.pl end
            return a.name:lower() < b.name:lower()
        end)
        for i = 1, math.min(30, #rows) do list[i] = rows[i] end
    end
    self.libLists[tab] = { n = n, list = list }
    return list
end

--- Ask Steam about a saved or recent server (players and ping come back through its event): the first time, and
--- again when nothing has answered 30 s later.
function PZMMMods:askHomeInfo(e)
    if e.inet or e.hosted or not self.homeSteam then return end
    local t = getTimestampMs()
    self.homeAsked = self.homeAsked or {}
    local key = tostring(e.host) .. ":" .. tostring(e.port)
    local at, info = self.homeAsked[key], PZMMMods.srvInfo[key]
    self.homeFirst = self.homeFirst or {}
    if not self.homeFirst[key] then self.homeFirst[key] = t end
    if not at or t - at > 30000 then
        self.homeAsked[key] = t
        pcall(function() steamRequestServerDetails(e.host, tonumber(e.port) or 16261) end)
    end
end

--- A saved or recent server that hasn't answered Steam 15 s after it was first asked.
function PZMMMods:homeDead(e)
    if e.hosted or e.inet or not self.homeSteam then return false end
    local key = tostring(e.host) .. ":" .. tostring(e.port)
    local first = self.homeFirst and self.homeFirst[key]
    if PZMMMods.srvFail[key] then return true end
    local info = PZMMMods.srvInfo[key]
    return first ~= nil and not (info and info.at) and getTimestampMs() - first > 15000
end

--- What's known of a library server: players, max, ping (Steam's answer, or the Internet list's own).
function PZMMMods:homeInfo(e)
    if e.inet and e.sv then
        local i = {}
        pcall(function() i.players, i.max, i.ping = tonumber(e.sv:getPlayers()) or 0, tonumber(e.sv:getMaxPlayers()) or 0, tonumber(e.sv:getPing()) or 0 end)
        return i
    end
    return PZMMMods.srvInfo[tostring(e.host) .. ":" .. tostring(e.port)]
end

--- Steam's answer for a server (players, ping, map, build), asking it (once per 30 s) when it hasn't answered.
function PZMMMods:srvInfoFor(ip, port)
    if self.homeSteam == nil then
        self.homeSteam = false
        pcall(function() self.homeSteam = getSteamModeActive() end)
        if self.homeSteam then listenResponded2() end
    end
    local host = hostOf(ip)
    self:askHomeInfo({ host = host, port = port })
    return PZMMMods.srvInfo[tostring(host) .. ":" .. tostring(port)]
end

--- Whether a saved, recent or hosted server shows under Servers' search and filters (the Internet list's own
--- are applied where it is built). What Steam hasn't answered yet can't match a filter on it.
function PZMMMods:rowPasses(name, ip, port, sv, modStr)
    local q = self.serverFilter and self.serverFilter ~= "" and self.serverFilter:lower() or nil
    if q and not tostring(name):lower():find(q, 1, true) and not tostring(ip):lower():find(q, 1, true) then return false end
    local f = self.sf or {}
    local any = false
    for _, v in pairs(f) do if v ~= nil then any = true; break end end
    if not any then return true end
    local info = self:srvInfoFor(ip, port) or {}
    local pl, mxp = info.players or 0, info.max or 0
    if f.players and pl < f.players then return false end
    if f.pw ~= nil then
        local pw = info.pw
        if pw == nil and sv then pcall(function() pw = sv:isPasswordProtected() and true or false end) end
        if pw == nil or (f.pw == "yes") ~= pw then return false end
    end
    if f.build and (info.ver or "") ~= f.build then return false end
    if f.notFull and mxp > 0 and pl >= mxp then return false end
    if f.notEmpty and pl <= 0 then return false end
    if f.map and not (info.mapFull or ""):lower():find(f.map, 1, true) then return false end
    local r = self:rulesFor({ ip = ip, port = port, host = hostOf(ip) })
    if f.pvp ~= nil and not (r and r.pvp ~= nil and r.pvp == f.pvp) then return false end
    if f.mods or f.maxdl then
        local str = modStr
        if str == nil then
            str = ""
            if sv then pcall(function() str = sv:getMods() or "" end) end
            if r and r.mods then str = r.mods end
        end
        local _, miss = self:modIds(str)
        if f.mods and not ((f.mods == "have" and #miss == 0) or (f.mods == "missing" and #miss > 0)) then return false end
        if f.maxdl and #miss > f.maxdl then return false end
    end
    return true
end

--- The chips of a server row from what Steam answered: players, ping, map, its mods against yours (`mods` =
--- { ids, missing }) and the PvP tag. Drawn from `tx` while they fit before `maxx`.
function PZMMMods:srvChips(info, r, mods, tx, cy, maxx, mx, my)
    local function put(label, c, tipText)
        local w = width(label) + 11
        if tx + w > maxx then return end
        local right = chip(self, label, tx, cy, c)
        if tipText and mx >= tx and mx < right and my >= cy and my < cy + H_SMALL + 2 then tip(self, tipText) end
        tx = right + 6
    end
    if info and info.players then put(info.players .. "/" .. tostring(info.max or "?"), nil, "Players\nOn the server now.") end
    if info and (info.ping or 0) > 0 then put(info.ping .. " ms", nil, "Ping\nHow long it takes to hear back from the server.") end
    if info and (info.map or "") ~= "" then put(info.map, nil, "Map\n" .. tostring(info.mapFull or info.map)) end
    if mods and #mods[1] > 0 then
        local a, b = mods[1], mods[2]
        put(#b == 0 and ("All " .. #a .. " mods installed") or (#b .. " of " .. #a .. " mods missing"), #b == 0 and C.ok or C.warn,
            #b == 0 and "Its mods\nYou have all of them." or ("Mods you don't have\n" .. table.concat(b, ", ")))
    end
    if r and r.pvp and tx + width("PvP") + 17 <= maxx then tx = tagChip(self, "PvP", tx, cy, false) + 6 end
end

--- Where each of a server's mods stands, as the app's chips: installed, on the Workshop (an item the server lists
--- is named for it), probably (a partial match, or the only missing mod and the only item you lack), not found.
--- Nothing for a mod that isn't installed while Steam's rules haven't come.
function PZMMMods:modStatuses(sel, r, ids)
    local c = sel.statC
    local t = getTimestampMs()
    local items = r and r.items and tostring(r.items) or ""
    if c and c.items == items and c.n == #ids and t - c.at < 1500 then return c.by end
    local have = {}
    pcall(function()
        local sids = getSteamWorkshopItemIDs()
        for i = 0, sids:size() - 1 do have[tostring(sids:get(i))] = true end
    end)
    for _, d in pairs(self.model.mods) do
        local w = wsOf(d)
        if w ~= "" then have[w] = true end
    end
    local titles, nun = {}, 0
    for it in items:gmatch("%d+") do
        if not have[it] then
            nun = nun + 1
            local st = self.steam and self.steam[it]
            titles[#titles + 1] = st and st.title and (tostring(st.title):lower():gsub("[^%w]", "")) or ""
        end
    end
    local nmiss = 0
    for _, id in ipairs(ids) do if not self.model.mods[id] then nmiss = nmiss + 1 end end
    local by = {}
    for _, id in ipairs(ids) do
        if self.model.mods[id] then
            by[id] = "installed"
        elseif r then
            local nid = (id:lower():gsub("[^%w]", ""))
            local state = "unknown"
            for _, ti in ipairs(titles) do
                if ti ~= "" and ti == nid then state = "found"; break end
                if #nid >= 4 and ti ~= "" and (ti:find(nid, 1, true) or (#ti >= 4 and nid:find(ti, 1, true))) then state = "likely" end
            end
            if state == "unknown" and nmiss == 1 and nun == 1 then state = "likely" end
            by[id] = state
        end
    end
    sel.statC = { items = items, n = #ids, at = t, by = by }
    return by
end

--- Pick a library server (or let go of it): Join takes its place in the play bar.
function PZMMMods:pickServer(e)
    if self.homeSel and self.homeSel.key == e.key then
        self.homeSel = nil
        return
    end
    self.homeSel = e
    if e.ip and not e.hosted then self:askRules(e) end
end

--- Join a library server: a saved one with its saved account, the rest through the game's connecting screen.
function PZMMMods:joinEntry(e)
    if e.sv and not e.inet then return self:joinSaved(e.sv) end
    local protected = true
    if e.sv then pcall(function() protected = e.sv:isPasswordProtected() end) end
    self:joinServer(e.name, e.ip, tostring(e.port), protected)
end

--- How many of a library server's mods you don't have (nil when its list isn't known yet), and how many it lists.
function PZMMMods:modsToGet(e)
    local str
    local r = (not e.hosted) and self:rulesFor(e) or nil
    if r and r.mods then str = r.mods end
    if str == nil and e.sv then pcall(function() str = e.sv:getMods() end) end
    if str == nil then return nil, 0 end
    if e.modStr ~= str then
        local ids, missing = self:modIds(str)
        e.modStr, e.nIds, e.nMissing = str, #ids, #missing
    end
    return e.nMissing, e.nIds
end

--- Get a server's missing Workshop mods: PZMM installs them (with the app), else their pages open in Steam.
function PZMMMods:getServerMods(e)
    local r = self:rulesFor(e)
    local items = r and r.items and tostring(r.items) or ""
    local have = {}
    pcall(function()
        local ids = getSteamWorkshopItemIDs()
        for i = 0, ids:size() - 1 do have[tostring(ids:get(i))] = true end
    end)
    for _, d in pairs(self.model.mods) do
        local w = wsOf(d)
        if w ~= "" then have[w] = true end
    end
    local want = {}
    for it in items:gmatch("%d+") do if not have[it] then want[#want + 1] = it end end
    if #want == 0 then
        -- its list isn't known (or you have all): its page in Servers has the details
        self:selectServer({ name = e.name, ip = e.ip, port = e.port, sv = e.sv, join = function() self:joinEntry(e) end })
        self.page = "servers"
        return
    end
    if self.app then
        self:request("install\t" .. table.concat(want, ","), #want .. (#want == 1 and " mod asked for." or " mods asked for."))
    else
        for i = 1, math.min(3, #want) do self:openWorkshop(want[i]) end
        if #want > 3 then self:say("Opened 3 of " .. #want .. ": its page in Servers lists the rest.") end
    end
end

--- The Check setup findings you haven't marked "It's fine", counted again only when the mods change.
function PZMMMods:homeIssues()
    local c, t = self.homeCheck, getTimestampMs()
    if c and c.ver == self.modVer and (not c.scanning or t - c.at < 500) then return c.n, c.scanning, c.failed end
    local ok, list = pcall(self.checkSetup, self)
    if not ok or type(list) ~= "table" then
        self.homeCheck = { ver = self.modVer, n = 0, failed = true, at = t }
        return 0, false, true
    end
    self.ignored = self.ignored or self:loadIgnored()
    local n = 0
    for _, f in ipairs(list) do if not self.ignored[f.key] then n = n + 1 end end
    self.homeCheck = { ver = self.modVer, n = n, scanning = list.scanning, at = t }
    return n, list.scanning, false
end

--- The enabled mods that ship Java while ZombieBuddy isn't running (0 when it is, or the scan isn't done).
function PZMMMods:zbNeeded()
    local zbOn = false
    if ZombieBuddy and ZombieBuddy.getVersion then pcall(function() zbOn = ZombieBuddy.getVersion() ~= nil end) end
    if zbOn then return 0 end
    local list, done = self:javaMods()
    if not done then return 0 end
    local on = 0
    for _, j in ipairs(list) do if j.d.isActive then on = on + 1 end end
    return on
end

-- the keys this screen answers to
K.SHORTCUTS = {
    { "/  or  Ctrl+F", "Search the mod list" },
    { "Ctrl+Z", "Undo the last change to your list" },
    { "Ctrl+Y  or  Ctrl+Shift+Z", "Redo it" },
    { "Ctrl+A", "Pick every mod the list shows" },
    { "Up, Down", "Step through the mods" },
    { "Alt+Up, Alt+Down", "Move the selected mod in the load order" },
    { "Space", "Enable or disable the selected mod" },
    { "Enter", "Open the first mod's details" },
    { "Page Up, Page Down", "Switch tab" },
    { "Esc", "Step back: close a menu, let go of a pick, then leave" },
}
--- Open Zomboid/Logs in the file manager.
function K.openLogs()
    pcall(function() showFolderInDesktop(Core.getMyDocumentFolder() .. getFileSeparator() .. "Logs") end)
end
--- What to paste into a bug report: versions, mod counts, folders and the PZMM app's state.
function K.diagnostics(ui, game)
    local on, n = 0, 0
    for _, d in pairs(ui.model.mods) do
        n = n + 1
        if d.isActive then on = on + 1 end
    end
    local folder = ""
    pcall(function() folder = Core.getMyDocumentFolder() end)
    local zb = "not running"
    if ZombieBuddy and ZombieBuddy.getVersion then pcall(function() zb = "v" .. tostring(ZombieBuddy.getVersion()) end) end
    local lines = {
        "PZMM in-game mod manager " .. (game ~= "" and game or "?"),
        "PZMM app: " .. (ui.app and (tostring(ui.app.version or "?") .. (ui:appUp() and " (running)" or " (installed, not running)")) or "not installed"),
        "Game build: " .. K.myBuild(),
        "Mods: " .. n .. " installed, " .. on .. " enabled, " .. (ui.nghost or 0) .. " enabled but not installed",
        "Load order problems: " .. (ui.oprobN or 0),
        "ZombieBuddy: " .. zb,
        "Zomboid folder: " .. folder,
        "Theme: " .. (THEME.key ~= "" and THEME.key or "default"),
    }
    return table.concat(lines, "\n")
end

--- A health pill ending at `right`: its icon, its words, a line, then its action (Fix, Run), all one button with a
--- tone (ok, bad, warn, idle). Returns its left edge.
function PZMMMods:healthPill(right, y, tone, icon, label, act, mx, my, fn)
    local c = tone == "ok" and C.ok or tone == "bad" and C.danger or tone == "warn" and C.warn or C.muted
    local lw = width(label)
    local aw = act ~= "" and width(act) or 0
    -- the app's .health: 12 px in, icon, 8, label, then the action (a rule, 10 px, its word, 14 px)
    local w = 12 + 14 + 8 + lw + (aw > 0 and (8 + 1 + 10 + aw + 14) or 14)
    local x = right - w
    local over = mx >= x and mx < right and my >= y and my < y + BAR
    round(self, x, y, w, BAR, 16, c, over and 0.16 or 0.08)
    outline(self, x, y, w, BAR, 16, c, over and 1 or 0.45)
    if icon then self:drawTextureScaled(icon, x + 12, y + math.floor((BAR - 14) / 2), 14, 14, 1, c.r, c.g, c.b) end
    text(self, label, x + 12 + 14 + 8, y + (BAR - H_SMALL) / 2, c)
    if aw > 0 then
        local dx = x + w - 14 - aw - 10 - 1
        if tone == "bad" or tone == "warn" then
            -- Fix: the segment filled in the tone, its word in the panel's colour
            roundEnds(self, dx, y, x + w - dx, BAR, 16, c, 1, false, true)
            bold(self, act, dx + 11, y + (BAR - H_SMALL) / 2, C.panel)
        else
            rect(self, dx, y + 1, 1, BAR - 2, c, over and 1 or 0.45)
            bold(self, act, dx + 11, y + (BAR - H_SMALL) / 2, c)
        end
    end
    self.feedHits[#self.feedHits + 1] = { kind = "fn", fn = fn, x = x, y = y, w = w, h = BAR }
    return x
end

--- Home's "Your mods with new updates": the top 5 of the feed, each with Update when PZMM is there to do it.
function PZMMMods:drawHomeUpdates(x, y, w, mx, my)
    bold(self, T("home_activity", "Your mods with new updates"), x, y, C.text, MEDIUM)
    local label = T("tab_feed", "What's new")
    local aw = width(label) + 28
    local ay = y + (H_MEDIUM - BAR) / 2
    local _, _, ao = flatButton(self, label, x + w - aw, ay, mx, my, "quiet")
    if ao then tip(self, "What's new\nChange notes for your Workshop mods updated since you last played.") end
    self.feedHits[#self.feedHits + 1] = { kind = "fn", fn = function() self.page, self.feedScroll = "feed", 0 end, x = x + w - aw, y = ay, w = aw, h = BAR }
    y = y + H_MEDIUM + 10
    local feed = self.feed or self:liteFeed()
    if not feed then
        text(self, self.steamState == "failed" and "Couldn't check for updates" or "Asking Steam...", x, y, C.muted)
        return y + H_SMALL + 24
    end
    local list = feed.entries
    if #list == 0 then
        text(self, "Nothing updated since you last played", x, y, C.muted)
        return y + H_SMALL + 24
    end
    local n = math.min(5, #list)
    local rh = H_SMALL * 2 + 24
    card(self, x, y, w, n * rh)
    for i = 1, n do
        local e = list[i]
        local ry = y + (i - 1) * rh
        local over = listRow(self, x, ry, w, rh, i == n, mx, my)
        local right = x + w - 16
        if self.app and e.pending then
            local ul = T("home_update", "Update")
            local uw = width(ul) + 48
            local uy = ry + (rh - BAR) / 2
            local _, _, uo = flatButton(self, ul, right - uw, uy, mx, my, "primary", TB.update)
            if uo then tip(self, "Update\nOpens PZMM, which updates it once you say yes.") end
            local item, title = e.item, e.title or e.item
            self.feedHits[#self.feedHits + 1] = { kind = "fn", fn = function() self:request("install\t" .. tostring(item), title .. " asked for.") end, x = right - uw, y = uy, w = uw, h = BAR }
            right = right - uw - 12
        end
        -- the first-letter tile (the Workshop picture is the app's), then the name, its new version and when
        local name = e.title or tostring(e.item)
        local ts = 32
        local ty0 = ry + math.floor((rh - ts) / 2)
        round(self, x + 16, ty0, ts, ts, 6, C.raised)
        outline(self, x + 16, ty0, ts, ts, 6, C.line, 1)
        local letter = name:sub(1, 1):upper()
        bold(self, letter, x + 16 + math.floor((ts - width(letter)) / 2), ty0 + math.floor((ts - H_SMALL) / 2), C.muted)
        local tx0 = x + 16 + ts + 12
        -- "May break saves" from the feed's risks
        if #(e.risks or {}) > 0 then
            local why = {}
            for _, r in ipairs(e.risks) do why[#why + 1] = K.RISKS[r] or r end
            local tl = T("feed_may_break", "May break saves")
            local cw0 = width(tl) + 11
            local cy0 = ry + math.floor((rh - H_SMALL - 2) / 2)
            chip(self, tl, right - cw0, cy0, C.danger)
            if mx >= right - cw0 and mx < right and my >= cy0 and my < cy0 + H_SMALL + 2 then tip(self, tl .. "\n" .. table.concat(why, "\n")) end
            right = right - cw0 - 10
        end
        bold(self, fit(name, right - tx0), tx0, ry + 10, over and C.onSelected or C.text)
        local sub = ((e.from and e.from ~= "" and e.to and e.to ~= "") and (e.from .. " -> " .. e.to .. "  ") or "") .. K.ago(e.updated)
        text(self, fit(sub, right - tx0), tx0, ry + 14 + H_SMALL, C.muted)
        self.feedHits[#self.feedHits + 1] = { kind = "fn", fn = function() self.page, self.feedScroll = "feed", 0 end, x = x, y = ry, w = w, h = rh }
    end
    y = y + n * rh
    if #list > n then
        text(self, "and " .. (#list - n) .. " more", x, y + 8, C.muted)
        y = y + H_SMALL + 8
    end
    return y + 24
end

--- Home's community links.
function PZMMMods:drawHomeCommunity(x, y, w, mx, my)
    bold(self, T("home_community", "Community"), x, y, C.text, MEDIUM)
    y = y + H_MEDIUM + 10
    local rh = H_SMALL + 20
    card(self, x, y, w, #K.COMMUNITY * rh)
    for i, l in ipairs(K.COMMUNITY) do
        local ry = y + (i - 1) * rh
        local over = listRow(self, x, ry, w, rh, i == #K.COMMUNITY, mx, my)
        text(self, l[1], x + 16, ry + 10, over and C.onSelected or C.text)
        local host = l[2]:gsub("^https?://", ""):gsub("^www%.", ""):gsub("/$", "")
        text(self, fit(host, w / 2 - 32), x + w - 16 - math.min(width(host), w / 2 - 32), ry + 10, C.muted)
        if over then tip(self, l[1] .. "\n" .. l[2]) end
        self.feedHits[#self.feedHits + 1] = { kind = "url", url = l[2], x = x, y = ry, w = w, h = rh }
    end
    return y + #K.COMMUNITY * rh + 24
end

--- Home's server library, as the app's: Popular / Favourites / Recent / Your servers with a search; picking a
--- server puts Join in the play bar, and the actions for it sit under the card. Returns the y under it.
function PZMMMods:drawHomeLibrary(x, y, w, mx, my, top, fh, bottom)
    bold(self, fit(T("home_pick", "Select a server, then click Join"), w), x, y, C.text, MEDIUM)
    y = y + H_MEDIUM + 10
    if self.homeSteam == nil then
        self.homeSteam = false
        pcall(function() self.homeSteam = getSteamModeActive() end)
        if self.homeSteam then listenResponded2() end
    end
    local tab = self.libTab or "popular"
    local noHost = #self:libraryList("hosted") == 0
    if tab == "hosted" and noHost then tab, self.libTab = "popular", nil end
    -- Popular needs Steam's Internet list: asked for once
    if tab == "popular" and #PZMMMods.inet == 0 and not self.popAsked and self.homeSteam then
        self.popAsked = true
        PZMMMods.listenInet()
        PZMMMods.inetAt = getTimestampMs()
        PZMMMods.askInet()
    end
    -- as tall as the column beside it (its cards end level), 440 at least
    local ch = math.max(440, (bottom or 0) - y)
    card(self, x, y, w, ch)
    local fx, fy, fw = x + 12, y + 12, w - 24
    round(self, fx, fy, fw, BAR, 6, C.raised)
    local he = self.homeSearch
    local focused = false
    pcall(function() focused = he:isFocused() end)
    outline(self, fx, fy, fw, BAR, 6, focused and C.focus or C.line, focused and C.focus.a or 1)
    if TB.search then self:drawTextureScaled(TB.search, fx + 10, fy + math.floor((BAR - 14) / 2), 14, 14, 1, C.muted.r, C.muted.g, C.muted.b) end
    he:setX(fx + 32)
    he:setY(fy + 4)
    he:setWidth(fw - 40)
    he:setVisible(fy >= top and fy + BAR <= top + fh)
    local q = he:getText():lower()
    -- the tabs, one segmented control
    local ty = fy + BAR + 10
    local labels = {}
    local lists = {}
    for i, t in ipairs(K.LIB_TABS) do
        local list = self:libraryList(t[1])
        local shown = {}
        if t[1] == "hosted" and noHost then list = {} end
        for _, e in ipairs(list) do
            if q == "" or e.name:lower():find(q, 1, true) then shown[#shown + 1] = e end
        end
        lists[t[1]] = shown
        local count = (t[1] == "popular" and #PZMMMods.inet == 0) and "" or (" " .. #shown)
        if not (t[1] == "hosted" and noHost) then labels[i] = t[2] .. count end
    end
    local tx0 = fx
    rect(self, fx, ty + BAR - 1, fw, 1, C.line, 1)
    for i, t in ipairs(K.LIB_TABS) do
      if labels[i] then
        local tw = width(labels[i]) + 24
        local on = tab == t[1]
        local over = mx >= tx0 and mx < tx0 + tw and my >= ty and my < ty + BAR
        if on or over then
            -- the main tab bar's: soft fill, thin accent sides, a 2 px underline
            round(self, tx0, ty, tw, BAR - 1, 4, C.accent, 0.32, true, false)
            rect(self, tx0 + 4, ty, tw - 8, 1, C.accent, 0.7)
            rect(self, tx0, ty + 4, 1, BAR - 5, C.accent, 0.7)
            rect(self, tx0 + tw - 1, ty + 4, 1, BAR - 5, C.accent, 0.7)
            rect(self, tx0, ty + BAR - 3, tw, 2, C.onSelected, 1)
        end
        text(self, labels[i], tx0 + 12, ty + (BAR - H_SMALL) / 2, (on or over) and C.onSelected or C.muted)
        local id = t[1]
        self.feedHits[#self.feedHits + 1] = { kind = "fn", fn = function() self.libTab, self.libScroll = id, 0 end, x = tx0, y = ty, w = tw, h = BAR }
        tx0 = tx0 + tw + 2
      end
    end
    -- the list: rows in a box of its own, scrolled
    local ly0 = ty + BAR + 8
    local foot = BAR + 12
    local lh = y + ch - ly0 - foot
    local rh = math.max(30, H_SMALL + 12)
    local list = lists[tab] or {}
    local full = #list * rh
    self.libScroll = math.max(0, math.min(self.libScroll or 0, math.max(0, full - lh)))
    self.libBox = { x = x, y = ly0, w = w, h = lh, full = full }
    local c0, c1 = math.max(ly0, top), math.min(ly0 + lh, top + fh)
    local mmy = (my >= ly0 and my < ly0 + lh) and my or -1e9
    if c1 > c0 then
        self:clearStencilRect()
        self:setStencilRect(x, c0, w, c1 - c0)
        local sel = self.homeSel
        for i, e in ipairs(list) do
            local ry = ly0 + (i - 1) * rh - self.libScroll
            if ry + rh > ly0 and ry < ly0 + lh then
                local picked = sel and sel.key == e.key
                local over = listRow(self, x + 8, ry, w - 16, rh, true, mx, mmy, picked)
                if not e.hosted then self:askHomeInfo(e) end
                local info = self:homeInfo(e)
                -- a dot: answered at all (green; a LAN or local server can answer in 0 ms), asking (grey, blinking),
                -- or not answering (a hollow ring, the name dimmed)
                local dc, st
                if e.hosted then
                    -- a hosted server answers on this PC's own port
                    e.opts = e.opts or self:hostedOptions(e.hosted, { "DefaultPort", "PublicDescription" })
                    local port = tonumber(e.opts.DefaultPort) or 16261
                    self:askHomeInfo({ host = "127.0.0.1", port = port })
                    if PZMMMods.srvInfo["127.0.0.1:" .. port] then dc = C.ok end
                elseif info and (info.at or e.inet) then dc = C.ok
                elseif self:homeDead(e) then st = "dead"
                elseif self.homeSteam then st = "ask" end
                local dy = ry + math.floor(rh / 2) - 4
                if dc then round(self, x + 22, dy, 8, 8, 4, dc)
                elseif st == "ask" then round(self, x + 22, dy, 8, 8, 4, C.muted, 0.35 + 0.65 * (math.sin(getTimestampMs() / 350) + 1) / 2)
                else outline(self, x + 22, dy, 8, 8, 4, C.muted, 1) end
                local nx = x + 40
                local pw = false
                if e.sv then pcall(function() pw = e.sv:isPasswordProtected() end) end
                if pw and TB.lock then
                    self:drawTextureScaled(TB.lock, nx, ry + math.floor((rh - 12) / 2), 12, 12, 1, C.muted.r, C.muted.g, C.muted.b)
                    nx = nx + 18
                end
                local get = self:modsToGet(e)
                local note = get and get > 0 and (get .. " to download") or nil
                local nw = note and width(note) + 12 or 0
                -- a players chip (Steam's answer, or the Internet list's own)
                local pcount = info and tonumber(info.players) and tonumber(info.max) and (info.players .. "/" .. info.max) or nil
                local pcw = pcount and (width(pcount) + 11 + 8) or 0
                if pcount and nx + 120 + nw + pcw < x + w - 24 then
                    local pcx = x + w - 24 - nw - pcw + 8
                    chip(self, pcount, pcx, ry + math.floor((rh - H_SMALL - 2) / 2), C.muted, over or picked)
                    nw = nw + pcw
                end
                text(self, fit(e.name ~= "" and e.name or (e.ip or ""), x + w - 24 - nx - nw), nx, ry + (rh - H_SMALL) / 2, (over or picked) and C.onSelected or (st == "dead" and copy(C.text, 0.55) or C.text))
                if note then text(self, note, x + w - 24 - width(note), ry + (rh - H_SMALL) / 2, C.warn) end
                if over then tip(self, picked and "Click again to go back to singleplayer" or (e.name .. (e.ip and ("\n" .. e.ip .. ":" .. tostring(e.port)) or ""))) end
                local hy0, hy1 = math.max(ry, ly0), math.min(ry + rh, ly0 + lh)
                local ex = e
                self.feedHits[#self.feedHits + 1] = { kind = "fn", fn = function() self:pickServer(ex) end, x = x + 8, y = hy0, w = w - 16, h = hy1 - hy0 }
            end
        end
        if #list == 0 then
            local msg = tab == "favourites" and "No favourites yet."
                or tab == "recent" and "Servers you join show up here."
                or tab == "hosted" and "You host no servers."
                or (not self.homeSteam and "The busiest servers need Steam running.")
                or (#PZMMMods.inet == 0 and "Getting the server list from Steam...")
                or "No servers match."
            text(self, fit(msg, w - 32), x + 16, ly0 + 10, C.muted)
        end
        self:clearStencilRect()
        self:setStencilRect(0, top, self.width, fh)
    end
    scrollbar(self, "libScroll", x, ly0, w, lh, full)
    -- its foot: the whole browser
    local label = T("srv_browse", "Browse servers")
    local bw = width(label) + 28
    local by = y + ch - foot + 6
    local _, _, bo = flatButton(self, label, x + 12, by, mx, my, "quiet")
    if bo then tip(self, "Servers\nFind servers, join, and give your hosted servers your mods.") end
    self.feedHits[#self.feedHits + 1] = { kind = "fn", fn = function() self.page, self.feedScroll = "servers", 0 end, x = x + 12, y = by, w = bw, h = BAR }
    y = y + ch + 12
    -- what to do with the picked server
    local sel = self.homeSel
    if sel then
        local desc, addr = "", nil
        if sel.hosted then
            sel.opts = sel.opts or self:hostedOptions(sel.hosted, { "DefaultPort", "PublicDescription" })
            desc = sel.opts.PublicDescription or ""
        else
            local r = self:rulesFor(sel)
            desc = (r and r.description) or ""
            if desc == "" and sel.sv then pcall(function() desc = sel.sv:getDescription() or "" end) end
            addr = tostring(sel.ip) .. ":" .. tostring(sel.port)
        end
        if sel.linkDesc ~= desc then sel.linkDesc, sel.discord, sel.site = desc, K.descLinks(desc) end
        -- its page in Servers, one card (what to do with it is in the Play button's menu)
        local ch2 = 64
        card(self, x, y, w, ch2)
        local over = mx >= x and mx < x + w and my >= y and my < y + ch2
        if over then
            round(self, x + 1, y + 1, w - 2, ch2 - 2, 7, C.hover)
            outline(self, x, y, w, ch2, 8, C.accent, 0.7)
        end
        local cc = over and C.onSelected or C.text
        if TB.server then self:drawTextureScaled(TB.server, x + 20, y + math.floor((ch2 - 20) / 2), 20, 20, 1, cc.r, cc.g, cc.b) end
        bold(self, T("home_server_info", "Browse server information"), x + 56, y + ch2 / 2 - H_SMALL - 1, cc)
        text(self, fit(T("home_server_info_sub", "Its mods and settings, in Servers"), w - 56 - 48), x + 56, y + ch2 / 2 + 2, C.muted)
        if K.CHEV_RIGHT then self:drawTextureScaled(K.CHEV_RIGHT, x + w - 34, y + math.floor((ch2 - 14) / 2), 14, 14, 1, cc.r, cc.g, cc.b) end
        if over then tip(self, "Servers\nIts mods and settings.") end
        self.feedHits[#self.feedHits + 1] = { kind = "fn", x = x, y = y, w = w, h = ch2, fn = function()
            if addr then
                self:selectServer({ name = sel.name, ip = sel.ip, port = sel.port, sv = sel.sv, join = function() self:joinEntry(sel) end })
            else
                self:selectServer({ name = sel.name, hosted = sel.hosted, key = sel.key })
            end
            self.page = "servers"
        end }
        y = y + ch2 - BAR
        y = y + BAR + 12
    else
        -- nothing on yet: the way to the Workshop
        local any = false
        for _, d in pairs(self.model.mods) do
            if d.isActive then any = true; break end
        end
        if not any then
            local msg = T("home_first_mods", "No mods on yet: pick some in the Workshop.")
            local wl = T("home_browse_workshop", "Browse Workshop")
            local bw = width(wl) + 28
            local hh = H_SMALL * 2 + 24 + BAR
            local lines = wrap(msg, w - 32)
            hh = #lines * (H_SMALL + 2) + 12 + BAR + 16
            card(self, x, y, w, hh)
            for i, l in ipairs(lines) do text(self, l, x + 16, y + 12 + (i - 1) * (H_SMALL + 2), C.muted) end
            local by2 = y + 12 + #lines * (H_SMALL + 2) + 6
            flatButton(self, wl, x + 16, by2, mx, my, "quiet")
            self.feedHits[#self.feedHits + 1] = { kind = "fn", fn = function() self.page, self.feedScroll = "workshop", 0 end, x = x + 16, y = by2, w = bw, h = BAR }
            y = y + hh + 12
        end
    end
    return y
end

function PZMMMods:drawHome(x, y, w, mx, my, top, fh)
    -- the hero and the play bar run the screen's width, joined to the tabs, as in the app
    local hh = math.min(400, math.max(350, math.floor(self.width * 0.27)))
    hh = math.min(hh, math.floor(self.height * 0.45))
    y = self:drawHero(0, y, self.width, hh, mx, my, top, fh)
    y = self:drawPlayBar(y, mx, my) + 24
    y = self:drawNotice(x, y, w, T("notice_home", "Featured from The Indie Stone and its news, the Workshop picture on each update, starting a hosted server, and its Stopped, Starting and Crashed states."), mx, my)
    local line = self.app and ("PZMM " .. tostring(self.app.version or "") .. (self:appUp() and " is running: what you change here reaches your profile." or " is installed: what you change here reaches your profile when it next starts."))
        or "The PZMM app adds news from The Indie Stone, change notes and Workshop pictures. Settings > About has the link."
    text(self, fit(line, w), x, y, C.muted)
    y = y + H_SMALL + 24
    local two = w >= 900
    local gap = 24
    local lw = two and math.floor((w - gap) / 2) or w
    local rw = two and (w - gap - lw) or w
    local ly = self:drawHomeUpdates(x, y, lw, mx, my)
    ly = self:drawHomeCommunity(x, ly, lw, mx, my)
    local ry = self:drawHomeLibrary(two and (x + lw + gap) or x, two and y or ly, rw, mx, my, top, fh, two and (ly - 24) or nil)
    return math.max(two and ly or 0, ry)
end

--- The play bar under the hero, as the app's: the main button (Play with a menu, or Join for the picked server) and
--- what it starts on the left, what matters at a glance on the right (each opens where it counts).
function PZMMMods:drawPlayBar(y, mx, my)
    local h = 84
    rect(self, 0, y, self.width, h, C.panel, 1)
    rect(self, 0, y + h - 1, self.width, 1, C.line, 1)
    if self.latest == nil then self.latest = getLatestSave() or false end
    local latest = self.latest or nil
    local has = latest and latest[1] and latest[2]
    local sel = self.homeSel
    local server = sel and not sel.hosted and sel or nil
    local info = server and self:homeInfo(server)
    local label, icon, fn, manage, hostedJoin
    if server then
        local pw = false
        if server.sv then pcall(function() pw = server.sv:isPasswordProtected() end) end
        label, icon = T("launcher_join", "Join"), pw and TB.lock or TB.play
        fn = function() self:joinEntry(server) end
    elseif sel then
        label, icon = T("home_manage", "Manage"), nil
        fn = function()
            self:selectServer({ name = sel.name, hosted = sel.hosted, key = sel.key })
            self.page = "servers"
        end
        manage = fn
        -- answering on its own port: Join (Start is the app's); otherwise Manage
        sel.opts = sel.opts or self:hostedOptions(sel.hosted, { "DefaultPort", "PublicDescription" })
        local hport = tonumber(sel.opts.DefaultPort) or 16261
        self:askHomeInfo({ host = "127.0.0.1", port = hport })
        if PZMMMods.srvInfo["127.0.0.1:" .. hport] then
            label, icon = T("launcher_join", "Join"), TB.play
            hostedJoin = true
            fn = function() self:joinServer(sel.name, "127.0.0.1", tostring(hport), true) end
        end
    else
        label, icon = T("home_play", "Play"), TB.play
        fn = function() self:gameScreen(has and "LATESTSAVE" or "SOLO") end
    end
    local bh = 46
    local bx, by = 32, y + (h - bh) / 2
    local lw = width(label, MEDIUM)
    local mw = 30 + (icon and 26 or 0) + lw + 30
    -- the caret is its own button, a 1 px gap in the bar's colour between them
    local cw = 36
    local over1 = mx >= bx and mx < bx + mw and my >= by and my < by + bh
    local over2 = mx >= bx + mw + 1 and mx < bx + mw + 1 + cw and my >= by and my < by + bh
    roundEnds(self, bx, by, mw, bh, 6, over1 and C.accentHover or C.accent, 1, true, false)
    roundEnds(self, bx + mw + 1, by, cw, bh, 6, over2 and C.accentHover or C.accent, 1, false, true)
    rect(self, bx + mw, by, 1, bh, C.panel, 1)
    local tx = bx + 30
    if icon then
        self:drawTextureScaled(icon, tx, by + math.floor((bh - 18) / 2), 18, 18, 1, C.onAccent.r, C.onAccent.g, C.onAccent.b)
        tx = tx + 26
    end
    bold(self, label, tx, by + (bh - H_MEDIUM) / 2, C.onAccent, MEDIUM)
    if TB.chev_down then self:drawTextureScaled(TB.chev_down, bx + mw + 1 + 11, by + math.floor((bh - 14) / 2), 14, 14, 1, C.onAccent.r, C.onAccent.g, C.onAccent.b) end
    if over1 then
        tip(self, server and ("Join\n" .. (server.name ~= "" and server.name or tostring(server.ip))) or sel and (hostedJoin and "Join\nIt's running on this PC." or "Manage\nIts page in Servers.") or (has and ("Play\nLoad " .. latest[1] .. " with its own mods.") or "Play\nThe game's new game screen."))
    end
    if over2 then tip(self, "More") end
    self.feedHits[#self.feedHits + 1] = { kind = "fn", fn = fn, x = bx, y = by, w = mw, h = bh }
    self.feedHits[#self.feedHits + 1] = { kind = "fn", x = bx + mw + 1, y = by, w = cw, h = bh, fn = function()
        local items = {}
        if server then
            local addr = tostring(server.ip) .. ":" .. tostring(server.port)
            local function link(label, url) return { label, function() openLink(url) end, icon = TB.external } end
            items[#items + 1] = { T("launcher_join_as_is", "Join as-is"), function() self:joinEntry(server) end, icon = TB.play }
            items[#items + 1] = { T("home_details", "Details"), function()
                self:selectServer({ name = server.name, ip = server.ip, port = server.port, sv = server.sv, join = function() self:joinEntry(server) end })
                self.page = "servers"
            end, icon = TB.info }
            items[#items + 1] = { T("home_copy_address", "Copy address"), function() Clipboard.setClipboard(addr) end }
            items[#items + 1] = link(T("launcher_battlemetrics", "BattleMetrics"), "https://www.battlemetrics.com/servers/zomboid?q=" .. urlEncode(tostring(server.ip)))
            if server.discord then items[#items + 1] = link(T("home_server_discord", "Server Discord"), server.discord) end
            if server.site then items[#items + 1] = link(T("home_server_site", "Server website"), server.site) end
            items[#items + 1] = { T("home_copy_link", "Copy join link"), function() Clipboard.setClipboard("pzmm://join/" .. addr) end, icon = TB.link }
            items[#items + 1] = false
            items[#items + 1] = { T("home_favourite", "Favourite"), function() self:toggleFavourite(server); self.libLists = nil end, K.favKey(self, server) and true or false, icon = TB.star }
            items[#items + 1] = false
        elseif sel then
            items[#items + 1] = { T("home_manage", "Manage"), manage, icon = TB.server }
            items[#items + 1] = false
        end
        if has then items[#items + 1] = { T("home_continue_latest", "Continue latest save"), function() self:gameScreen("LATESTSAVE") end } end
        items[#items + 1] = { T("home_new_game", "New game"), function() self:gameScreen("SOLO") end }
        items[#items + 1] = { T("home_load_save", "Load a save"), function() self:gameScreen("LOAD") end }
        items[#items + 1] = { T("home_join_server", "Join a server"), function() self:gameScreen("MULTIPLAYER") end }
        items[#items + 1] = false
        local profiles = self.app and self:profileItems()
        if profiles then
            items[#items + 1] = { T("home_switch_profile", "Switch profile"), heading = true }
            for _, p in ipairs(profiles) do items[#items + 1] = p end
            items[#items + 1] = false
        end
        items[#items + 1] = { T("tab_settings", "Settings"), function() self.page, self.feedScroll = "settings", 0 end, icon = TB.settings }
        self:menu(items, bx + mw + 1, by + bh)
    end }

    -- what it starts: the last save or server played, or the picked server's players
    local lx = bx + mw + 1 + cw + 28
    local function leftStat(slabel, value, vc, tipText, onClick, bar)
        bold(self, slabel, lx, y + h / 2 - H_SMALL - 2, C.muted)
        local sw = width(value)
        local sover = mx >= lx and mx < lx + sw + 6 and my >= y + h / 2 and my < y + h / 2 + H_SMALL + 2
        text(self, value, lx, y + h / 2 + 2, sover and C.text or vc)
        if sover then
            rect(self, lx, y + h / 2 + 2 + H_SMALL, sw, 1, C.text, 1)
            tip(self, tipText)
        end
        local ex = lx + sw
        if bar then
            round(self, lx + sw + 8, y + h / 2 + 2 + math.floor((H_SMALL - 6) / 2), 48, 6, 3, C.line)
            if bar > 0 then round(self, lx + sw + 8, y + h / 2 + 2 + math.floor((H_SMALL - 6) / 2), math.max(6, math.floor(48 * bar)), 6, 3, C.accentText) end
            ex = ex + 56
        end
        if onClick then self.feedHits[#self.feedHits + 1] = { kind = "fn", fn = onClick, x = lx, y = y + h / 2, w = sw + 6, h = H_SMALL + 4 } end
        lx = ex + 28
    end
    local rightStats = {}
    if server then
        local pl, mxp = info and tonumber(info.players), info and tonumber(info.max)
        leftStat("PLAYERS", (pl and mxp) and (pl .. "/" .. mxp) or "-", C.text, "Players\nOn the server now.", nil, (pl and mxp and mxp > 0) and math.min(1, pl / mxp) or nil)
        local function g(f)
            local ok, v = pcall(f)
            return ok and v or nil
        end
        local r = self:rulesFor(server)
        local ping = info and tonumber(info.ping)
        local map = server.sv and g(function() return server.sv:getMapName() end)
        local build = (r and r.version) or (server.sv and g(function() return server.sv:getVersion() end))
        map = tostring(map or ""):match("^[^;]+") or ""
        rightStats = {
            { "PING", (ping and ping > 0) and (ping .. " ms") or "-", "Ping\nHow long it takes to hear back from the server." },
            { "MAP", map ~= "" and map or "-", "Map\n" .. (map ~= "" and map or "Not known yet.") },
            { "BUILD", (build and tostring(build) ~= "") and tostring(build) or "-", "Build\nThe game version the server runs." },
        }
    elseif sel then
        -- a server hosted here: asked on this PC's own port, as any other
        sel.opts = sel.opts or self:hostedOptions(sel.hosted, { "DefaultPort", "PublicDescription" })
        local port = tonumber(sel.opts.DefaultPort) or 16261
        self:askHomeInfo({ host = "127.0.0.1", port = port })
        local hi = PZMMMods.srvInfo["127.0.0.1:" .. port]
        sel.up = hi ~= nil
        sel.dead = self:homeDead({ host = "127.0.0.1", port = port })
        leftStat("PLAYERS", sel.up and (hi.players .. "/" .. hi.max) or "-", C.text, "Players\nOn the server now.")
        rightStats = {
            { "PORT", tostring(port), "Port\nWhere players join it." },
            { "BUILD", "42", "Build\nThe game version this screen runs: hosted servers start from the app (Start is there)." },
        }
    else
        -- the save played last, or the server joined after it
        local rs = self:recentServers()
        local lastSrv = rs[1]
        local asServer = lastSrv and ((self.lastPlayed and math.abs(lastSrv.at - self.lastPlayed) < 600) or (not self.lastPlayed and not has))
        if asServer then
            local ip, port = lastSrv.addr:match("^(.*):(%d+)$")
            if not ip then ip, port = lastSrv.addr, "16261" end
            local name = lastSrv.name ~= "" and lastSrv.name or lastSrv.addr
            leftStat("LAST SERVER PLAYED", fit(name, 220), C.text, name .. "\n" .. K.ago(lastSrv.at), function()
                self:pickServer({ name = lastSrv.name, ip = ip, port = port, host = hostOf(ip), recent = lastSrv, key = ip .. ":" .. port })
            end)
        else
            if has then
                leftStat("LAST PLAYED", fit(T("home_continue", "Continue %1", latest[1]), 220), C.text, "Saves\nEvery save, with Play, Folder and Delete.", function() self.page, self.feedScroll = "saves", 0 end)
            else
                -- nothing played yet: the way to a first server
                local fl = T("home_find_server", "Find a server")
                local fw = width(fl) + 20 + 28
                local fy = y + math.floor((h - BAR) / 2)
                local _, _, fo = flatButton(self, fl, lx, fy, mx, my, "quiet", TB.search)
                if fo then tip(self, "Servers\nFind servers, join, and give your hosted servers your mods.") end
                self.feedHits[#self.feedHits + 1] = { kind = "fn", fn = function() self.page, self.feedScroll = "servers", 0 end, x = lx, y = fy, w = fw, h = BAR }
                lx = lx + fw + 28
            end
        end
        local on, bad = 0, 0
        for _, d in pairs(self.model.mods) do
            if d.isActive then on = on + 1 end
            if SHOWN.Problems(d) then bad = bad + 1 end
        end
        bad = bad + (self.nghost or 0)
        local feed = self.feed or self:liteFeed()
        local updates = feed and #feed.entries
        -- with PZMM's feed: the updates still waiting to download, as the app counts them
        if self.feed then
            updates = 0
            for _, e in ipairs(self.feed.entries) do if e.pending then updates = updates + 1 end end
        end
        rightStats = {
            { "MODS", tostring(on), "Mods\nYour enabled mods.", function() self.page, self.tab = "mods", "Enabled"; self:updateView() end },
            { "PROBLEMS", tostring(bad), "Problems\nMods that can't load on this build or clash with another.", function() self.page, self.tab = "mods", "Problems"; self:updateView() end, bad > 0 and C.danger },
            { "UPDATES", updates and tostring(updates) or "...", (self.feed and "Updates\nUpdates waiting to download: Update in What's new gets them." or "Updates\nYour Workshop mods updated since you last played.") .. (updates and "" or " Asking Steam..."), function() self.page, self.feedScroll = "feed", 0 end, updates and updates > 0 and C.warn },
        }
    end

    -- the pill ends the bar: the server's mods, or ZombieBuddy, or the setup check; one button with its action
    local py = y + math.floor((h - BAR) / 2)
    local right = self.width - 32
    local pleft
    if server then
        local get, total = self:modsToGet(server)
        local function details() self:selectServer({ name = server.name, ip = server.ip, port = server.port, sv = server.sv, join = function() self:joinEntry(server) end }); self.page = "servers" end
        self:askHomeInfo(server)
        if get == nil and self:homeDead(server) then
            pleft = self:healthPill(right, py, "bad", TB.alert, T("home_not_answering", "Not answering"), T("home_details", "Details"), mx, my, details)
        elseif get == nil then
            pleft = self:healthPill(right, py, "idle", TB.pulse, "Finding its mods...", T("home_details", "Details"), mx, my, details)
        elseif get > 0 then
            pleft = self:healthPill(right, py, "warn", TB.download, get .. (get == 1 and " mod to download" or " mods to download"), T("home_get_mods", "Get mods"), mx, my, function() self:getServerMods(server) end)
        else
            pleft = self:healthPill(right, py, "ok", TB.check, total > 0 and (total == 1 and "Its mod is ready" or ("All " .. total .. " mods ready")) or "Vanilla", T("home_details", "Details"), mx, my, details)
        end
    elseif sel then
        pleft = self:healthPill(right, py, sel.up and "ok" or "idle", sel.up and TB.check or TB.pulse, sel.up and "Running" or not self.homeSteam and "Unknown" or sel.dead and T("home_not_answering", "Not answering") or "Checking...", T("home_manage", "Manage"), mx, my, fn)
    else
        local zb = self:zbNeeded()
        local function open() self.page, self.tab, self.checkOpen, self.findings = "mods", "All", true, nil; self:updateView() end
        if zb > 0 then
            pleft = self:healthPill(right, py, "bad", TB.alert, zb .. (zb == 1 and " mod needs ZombieBuddy" or " mods need ZombieBuddy"), T("home_fix", "Fix"), mx, my, function() self.page, self.feedScroll = "java", 0 end)
        else
            local n, scanning, failed = self:homeIssues()
            if failed then
                pleft = self:healthPill(right, py, "bad", TB.alert, T("home_health_failed", "Check setup couldn't run"), T("home_run", "Run"), mx, my, open)
            elseif scanning and n == 0 then
                pleft = self:healthPill(right, py, "idle", TB.pulse, T("app_checking", "Checking..."), "", mx, my, open)
            elseif n == 0 then
                pleft = self:healthPill(right, py, "ok", TB.check, T("home_health_ok", "Check setup: all set"), T("home_run", "Run"), mx, my, open)
            else
                pleft = self:healthPill(right, py, "bad", TB.alert, n == 1 and "Check setup: 1 problem" or ("Check setup: " .. n .. " problems"), T("home_fix", "Fix"), mx, my, open)
            end
        end
    end
    if mx >= pleft and mx < right and my >= py and my < py + BAR then
        tip(self, server and "Its mods\nAgainst the mods you have: joining downloads the Workshop ones." or "Check setup\nOpens My mods with Check setup: what can't load, what's missing, what clashes.")
    end
    local sx = pleft - 28
    for i = #rightStats, 1, -1 do
        local st = rightStats[i]
        local cw2 = math.max(width(st[1]) + 1, width(st[2]))
        sx = sx - cw2
        local sy = y + h / 2 - H_SMALL - 2
        local o = mx >= sx - 6 and mx < sx + cw2 + 6 and my >= y + 8 and my < y + h - 8
        bold(self, st[1], sx, sy, C.muted)
        text(self, st[2], sx, y + h / 2 + 2, st[5] or C.text)
        if o then
            if st[4] then rect(self, sx, y + h / 2 + 2 + H_SMALL, width(st[2]), 1, C.text, 1) end
            tip(self, st[3])
        end
        if st[4] then self.feedHits[#self.feedHits + 1] = { kind = "fn", fn = st[4], x = sx - 6, y = y + 8, w = cw2 + 12, h = h - 16 } end
        sx = sx - 28
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
        local art = (vp and vp.isActive and K.HERO_VP) or K.HERO_PZ
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
    local logo = (vp and vp.isActive and K.VIEWPOINT_LOGO) or K.LOGO_PZ or (main and (main.pzmmLogo or main.logoTexture))
    local title = self.homeSel and self.homeSel.name ~= "" and self.homeSel.name or "Singleplayer"
    local ty = y + h - 22 - H_MARKER
    if logo then
        local lw = math.min(380, w * 0.5)
        local lh = lw * logo:getHeight() / logo:getWidth()
        self:drawTextureScaled(logo, x + 32, ty - 6 - lh, lw, lh, 1, 1, 1, 1)
    end
    text(self, title, x + 34, ty + 2, hex("000000", 0.6), MARKER)
    text(self, title, x + 32, ty, hex("ffffff"), MARKER)

    -- the community note, tilted, its words in the texture: the hovered link swaps in its blood version
    local nw, nh = K.NOTE:getWidth(), K.NOTE:getHeight()
    local nx, ny = x + w - 40 - nw, y + 22
    -- the mouse on the flat paper: undo the tilt about the paper's centre
    local cx, cy = nx + nw / 2, ny + nh / 2
    local dx, dy = mx - cx, my - cy
    local px = K.NOTE_W / 2 + dx * math.cos(K.NOTE_TILT) - dy * math.sin(K.NOTE_TILT)
    local py = K.NOTE_H / 2 + dx * math.sin(K.NOTE_TILT) + dy * math.cos(K.NOTE_TILT)
    local hover
    for i, l in ipairs(K.COMMUNITY) do
        local b = l[3]
        if px >= b[1] and px < b[1] + b[3] and py >= b[2] and py < b[2] + b[4] then hover = i end
    end
    image(self, hover and K.NOTE_HOVER[hover] or K.NOTE, nx, ny, nw, nh)
    if hover then
        local l = K.COMMUNITY[hover]
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
    end
    return self.mapOrder
end
--- The game's own map: mod maps load before it, so it stays last (as the app's map order).
K.BASE_MAP = "Muldraugh, KY"

--- `maps` with the game's own map last, the rest in their order.
local function baseLast(maps)
    local out, base = {}, false
    for _, m in ipairs(maps) do
        if m == K.BASE_MAP then base = true else out[#out + 1] = m end
    end
    if base then out[#out + 1] = K.BASE_MAP end
    return out
end

--- A map folder's title and the maps it builds on (its map.info: title=, lots=), read once.
function PZMMMods:mapInfo(folder)
    self.mapInfos = self.mapInfos or {}
    if self.mapInfos[folder] == nil then
        local e = { title = folder, lots = {} }
        pcall(function()
            local info = getMapInfo(folder)
            if info then
                e.title = info.title or folder
                e.desc, e.thumb = info.description, info.thumb
                for _, l in ipairs(info.lots or {}) do e.lots[#e.lots + 1] = l end
            end
        end)
        self.mapInfos[folder] = e
    end
    return self.mapInfos[folder]
end

--- Autosort maps, as the app: a map whose lots= names another listed map builds on it, so it goes just before
--- that map; the rest keep their order; Muldraugh last. Maps that build on each other in a loop keep theirs.
function PZMMMods:autosortMaps()
    local out = baseLast(self:maps())
    for _ = 0, #out do
        local moved = false
        for i = 1, #out do
            local lots = self:mapInfo(out[i]).lots
            for j = 1, i - 1 do
                local hit = false
                for _, l in ipairs(lots) do if l == out[j] then hit = true end end
                if hit then
                    table.insert(out, j, table.remove(out, i))
                    moved = true
                    break
                end
            end
        end
        if not moved then
            self:arrangeMaps(out, "Maps autosorted: each before the maps it builds on, Muldraugh last.")
            return
        end
    end
    self:say("These maps build on each other in a loop, so their order was kept.")
end

--- Make `maps` the map order (the game's own map last), saved with Accept.
function PZMMMods:arrangeMaps(maps, said)
    self:mapRemember()
    self.mapOrder = baseLast(maps)
    self:setMapOrder(self.mapOrder)
    self.diffAt = nil
    if said then self:say(said) end
end

--- Write `order` to the game's map order (saved with Accept, as the game's own Map order screen does).
function PZMMMods:setMapOrder(order)
    pcall(function()
        local mo = self.model:getActiveMods():getMapOrder()
        mo:clear()
        for _, m in ipairs(order) do mo:add(m) end
    end)
end
--- The map at `from` moved to place `to` (Up, Down, Top); the game's own map stays last.
function PZMMMods:moveMap(from, to)
    local o = {}
    for i, m in ipairs(self.mapOrder) do o[i] = m end
    if o[from] == K.BASE_MAP then return end
    table.insert(o, to, table.remove(o, from))
    self:arrangeMaps(o)
end

-- Map mods and the cell grid ----------------------------------------------------------------------------

-- a fixed palette for the grid (no red: overlaps are drawn in the danger red)
K.MAP_COLORS = { hex("58a6ff"), hex("3fb950"), hex("e3b341"), hex("bc8cff"), hex("39c5cf"), hex("ff7eb6"),
    hex("9ccc65"), hex("d2a679"), hex("7986cb"), hex("4db6ac"), hex("b0bec5"), hex("8d6e63") }
-- the grid's one colour for every map, unless "Per-map colours" is on
K.MAP_CELL = hex(THEME.key:sub(-1) == "L" and "2f6fae" or "5b9bd5")
local function mapColor(ui, e) return ui.mapColours and K.MAP_COLORS[e.color] or K.MAP_CELL end

--- Every installed map mod (one with media/maps folders: getMapFoldersForMod), found 40 mods a frame, and then
--- each map folder's cells from its file names (B42 world_X_Y.lotpack, B41 X_Y.lotheader), 500 files a frame.
--- { mods = { { d, entries } }, entries = { e }, owners = { cell -> { e } }, multi = { cells with 2+ maps },
--- ver, found, finished }; an entry e = { d, folder, key, cells, n, done, color }. A cell is (x + 500) * 10000 + y + 500.
function PZMMMods:mapScan()
    local s = self.mapScanState
    if not s then
        s = { todo = {}, ti = 1, mods = {}, modOf = {}, entries = {}, owners = {}, multi = {}, queue = {}, qi = 1, job = nil, ver = 0 }
        for _, d in pairs(self.model.mods) do s.todo[#s.todo + 1] = d end
        table.sort(s.todo, byName)
        self.mapScanState = s
    end
    if s.finished then return s end
    for _ = 1, 40 do
        local d = s.todo[s.ti]
        if not d then break end
        s.ti = s.ti + 1
        pcall(function()
            local a = getMapFoldersForMod(d.modId)
            if a and a:size() > 0 then
                local m = { d = d, entries = {} }
                for i = 0, a:size() - 1 do
                    local f = tostring(a:get(i))
                    local e = { d = d, folder = f, key = d.modId .. "\t" .. f, cells = {}, set = {}, n = 0, done = false, color = (#s.entries % #K.MAP_COLORS) + 1 }
                    m.entries[#m.entries + 1] = e
                    s.entries[#s.entries + 1] = e
                    s.queue[#s.queue + 1] = e
                end
                s.mods[#s.mods + 1] = m
                s.modOf[d.modId] = m
                s.ver = s.ver + 1
            end
        end)
    end
    s.found = s.ti > #s.todo
    local budget = 500
    while budget > 0 do
        local j = s.job
        if not j then
            local e = s.queue[s.qi]
            if not e then break end
            s.qi = s.qi + 1
            local ok, list = pcall(function() return listFilesInModDirectory(e.d.modId, "media/maps/" .. e.folder) end)
            if ok and list then
                j = { e = e, list = list, i = 0, n = list:size() }
                s.job = j
            else
                e.done, e.set = true, nil
            end
        end
        if j then
            local e = j.e
            local last = math.min(j.n - 1, j.i + budget - 1)
            budget = budget - (last - j.i + 1)
            local ok = pcall(function()
                for i = j.i, last do
                    local n = tostring(j.list:get(i)):gsub("\\", "/"):match("([^/]+)$") or ""
                    local x, y = n:match("^world_(%-?%d+)_(%-?%d+)%.lotpack$")
                    if not x then x, y = n:match("^(%-?%d+)_(%-?%d+)%.lotheader$") end
                    if x then
                        x, y = tonumber(x), tonumber(y)
                        if x > -499 and x < 9499 and y > -499 and y < 9499 then
                            local k = (x + 500) * 10000 + (y + 500)
                            if not e.set[k] then
                                e.set[k] = true
                                e.cells[#e.cells + 1] = k
                                local o = s.owners[k]
                                if o then
                                    o[#o + 1] = e
                                    if #o == 2 then s.multi[#s.multi + 1] = k end
                                else
                                    s.owners[k] = { e }
                                end
                            end
                        end
                    end
                end
            end)
            j.i = last + 1
            s.ver = s.ver + 1
            if not ok or j.i >= j.n then
                e.done, e.n, e.set = true, #e.cells, nil
                s.job = nil
            end
        end
    end
    s.finished = s.found and s.qi > #s.queue and s.job == nil
    return s
end

--- Whether a map is drawn on the grid: what you toggled, else whether its mod is on.
function PZMMMods:mapShown(e)
    local v = self.mapShow[e.key]
    if v ~= nil then return v end
    return e.d.isActive and true or false
end

--- Whether mod `a` needs mod `b` (an add-on over its base is no clash).
local function needsMod(a, b)
    if a.requireMods and a.requireMods[b.modId] then return true end
    for _, q in ipairs(rules(a).requires) do if q == b.modId then return true end end
    return false
end

--- The clashes of enabled map mods, hidden maps counted too: shared cells (not between a mod and what it
--- requires), and the game's own map conflicts. modId -> { others = { modId -> cells }, extra = { name -> true } }.
function PZMMMods:mapClashes()
    local s = self:mapScan()
    local c = self.mapClash
    if c and c.ver == s.ver and c.mv == self.modVer then return c.by end
    local by = {}
    local function get(d)
        by[d.modId] = by[d.modId] or { others = {}, extra = {} }
        return by[d.modId]
    end
    local function pair(a, b, n)
        if a == b or not a.isActive or not b.isActive or needsMod(a, b) or needsMod(b, a) then return end
        local ga, gb = get(a), get(b)
        ga.others[b.modId] = (ga.others[b.modId] or 0) + n
        gb.others[a.modId] = (gb.others[a.modId] or 0) + n
    end
    for _, k in ipairs(s.multi) do
        local o = s.owners[k]
        for i = 1, #o do
            for j = i + 1, #o do pair(o[i].d, o[j].d, 1) end
        end
    end
    -- the game's own check, by folder (the maps it can't place, or that hide each other)
    -- (cached on the mods and the scan's entries: a half-finished scan has found fewer maps)
    local gkey = tostring(self.modVer) .. "|" .. #s.entries .. "|" .. tostring(s.found) .. "|" .. tostring(s.finished)
    if not (c and c.gkey == gkey and c.game) then
        local game = {}
        for _, e in ipairs(s.entries) do
            if e.d.isActive then
                pcall(function()
                    local l = self.model:getMapConflicts(e.folder)
                    for j = 0, l:size() - 1 do
                        game[#game + 1] = { e, tostring(l:get(j)) }
                    end
                end)
            end
        end
        c = { game = game, mv = self.modVer, gkey = gkey }
    end
    for _, g in ipairs(c.game) do
        local e, folder = g[1], g[2]
        local found = false
        for _, o in ipairs(s.entries) do
            if o.folder == folder and o.d ~= e.d and o.d.isActive then
                found = true
                if not needsMod(e.d, o.d) and not needsMod(o.d, e.d) then
                    local ga = get(e.d)
                    ga.others[o.d.modId] = ga.others[o.d.modId] or 0
                end
            end
        end
        if not found and folder ~= e.folder then get(e.d).extra[self:mapInfo(folder).title] = true end
    end
    c.ver, c.by = s.ver, by
    self.mapClash = c
    return by
end

--- The names a map mod clashes with (as strings, with the shared cells), empty when none.
function PZMMMods:mapClashList(d)
    local by = self:mapClashes()[d.modId]
    local out = {}
    if not by or not d.isActive then return out end
    for id, n in pairs(by.others) do
        local o = self.model.mods[id]
        out[#out + 1] = (o and o.name or id) .. (n > 0 and (" (" .. n .. (n == 1 and " cell)" or " cells)")) or "")
    end
    for name in pairs(by.extra) do out[#out + 1] = name end
    table.sort(out)
    return out
end

--- What the grid draws: every cell of the shown maps, with the colour of its map (0: two or more maps).
--- Rebuilt when the scan, the mods or what's shown change (at most every 300 ms while scanning).
function PZMMMods:gridCells()
    local s = self:mapScan()
    local g = self.gridCache
    if g and g.mv == self.modVer and g.sv == self.showVer then
        if g.ver == s.ver then return g end
        if not s.finished and getTimestampMs() - g.at < 300 then return g end
    end
    local cnt, col = {}, {}
    local shown = 0
    for _, e in ipairs(s.entries) do
        if self:mapShown(e) then
            shown = shown + 1
            for _, k in ipairs(e.cells) do
                local n = cnt[k]
                if n then
                    cnt[k] = n + 1
                    col[k] = 0
                else
                    cnt[k] = 1
                    col[k] = e.color
                end
            end
        end
    end
    local xs, ys, cs, over = {}, {}, {}, 0
    local x0, x1, y0, y1 = 1e9, -1e9, 1e9, -1e9
    for k, n in pairs(cnt) do
        local x = math.floor(k / 10000) - 500
        local y = k % 10000 - 500
        local i = #xs + 1
        xs[i], ys[i], cs[i] = x, y, col[k]
        if n > 1 then over = over + 1 end
        if x < x0 then x0 = x end
        if x > x1 then x1 = x end
        if y < y0 then y0 = y end
        if y > y1 then y1 = y end
    end
    g = { ver = s.ver, mv = self.modVer, sv = self.showVer, at = getTimestampMs(), xs = xs, ys = ys, cs = cs, n = #xs, over = over, shown = shown, x0 = x0, x1 = x1, y0 = y0, y1 = y1 }
    self.gridCache = g
    return g
end

--- The map mods the list shows: filtered by the search and the filter, sorted; rebuilt when any of that changes.
function PZMMMods:mapRows()
    local s = self:mapScan()
    local ckey = s.ver .. "|" .. (self.modVer or 0) .. "|" .. (self.mapQuery or "") .. "|" .. (self.mapFilter or "all") .. "|" .. (self.mapSort or "order") .. "|" .. (self.cellVer or 0) .. "|" .. (self.steamState or "") .. "|" .. tostring(self.mapHideSpawn ~= false)
    if self.mapRowsKey == ckey then return self.mapRowsList end
    local q = self.mapQuery and self.mapQuery:lower()
    -- the mods with a map at a picked cell
    local atCell
    if self.gridSel then
        atCell = {}
        for _, k in ipairs(self.gridSel.keys) do
            for _, e in ipairs(s.owners[k] or {}) do atCell[e.d.modId] = true end
        end
    end
    local rows, nclash = {}, {}
    for _, m in ipairs(s.mods) do
        local d = m.d
        local n = #self:mapClashList(d)
        nclash[m] = n
        local ok = true
        if q then
            ok = d.name:lower():find(q, 1, true) or d.modId:lower():find(q, 1, true)
            if not ok then
                for _, e in ipairs(m.entries) do
                    if e.folder:lower():find(q, 1, true) or self:mapInfo(e.folder).title:lower():find(q, 1, true) then ok = true end
                end
            end
            -- its Workshop ID and its author too
            if not ok then
                if wsOf(d):find(q, 1, true) then ok = true end
                local au = ""
                pcall(function() au = tostring(d.modInfo:getAuthor() or "") end)
                if au:lower():find(q, 1, true) then ok = true end
            end
        end
        local f = self.mapFilter
        if f == "on" and not d.isActive then ok = false end
        -- hidden mods are out of the list (Unhide brings them back); spawn-point maps (no cells of their own) too, unless shown
        if self.hidden[d.modId] then ok = false end
        if ok and self.mapHideSpawn ~= false and K.spawnOnly(m) then ok = false end
        if ok and (f ~= "clash" or n > 0) and (not atCell or atCell[d.modId]) then rows[#rows + 1] = m end
    end
    local by = self.mapSort or "order"
    -- the numbers a sort needs, worked out once per mod
    local key = {}
    if by == "order" or by == "cells" or by == "updated" or by == "newest" then
        local at = {}
        for i, f in ipairs(self:maps()) do at[f] = i end
        for _, m in ipairs(rows) do
            local v = 0
            if by == "order" then
                v = 1e9
                for _, e in ipairs(m.entries) do v = math.min(v, at[e.folder] or 1e9) end
            elseif by == "cells" then
                for _, e in ipairs(m.entries) do v = v + #e.cells end
            else
                local st = self:steamOf(m.d)
                v = st and (by == "updated" and st.updated or st.created) or 0
            end
            key[m] = v
        end
    end
    table.sort(rows, function(a, b)
        -- spawn-point maps after the maps, in every sort
        local sa, sb = K.spawnOnly(a), K.spawnOnly(b)
        if sa ~= sb then return sb end
        if by == "clash" and nclash[a] ~= nclash[b] then return nclash[a] > nclash[b] end
        if by == "order" then
            if key[a] ~= key[b] then return key[a] < key[b] end
        elseif key[a] and key[a] ~= key[b] then
            return key[a] > key[b]
        end
        if (by == "enabled" or by == "clash") and a.d.isActive ~= b.d.isActive then return a.d.isActive end
        return byName(a.d, b.d)
    end)
    self.mapRowsKey, self.mapRowsList = ckey, rows
    return rows
end

--- A map mod with no cells of its own (its scan is done): spawn points for a vanilla town, or an add-on.
function K.spawnOnly(m)
    if #m.entries == 0 then return false end
    for _, e in ipairs(m.entries) do
        if not e.done or #e.cells > 0 then return false end
    end
    return true
end

--- The text of Copy: the enabled maps' mods (with what they need, in load order), Workshop items, Map= and links.
function PZMMMods:mapCopy(kind)
    local s = self:mapScan()
    if not s.found then return self:say("Still looking for map mods: try again in a moment.") end
    local order = self:maps()
    local mods, seen = {}, {}
    local function add(d)
        if seen[d.modId] then return end
        seen[d.modId] = true
        for _, q in ipairs(rules(d).requires) do
            local r = self.model.mods[q]
            if r then add(r) end
        end
        mods[#mods + 1] = d
    end
    local links = {}
    for _, f in ipairs(order) do
        for _, m in ipairs(s.mods) do
            for _, e in ipairs(m.entries) do
                if e.folder == f and m.d.isActive and not seen["link" .. m.d.modId] then
                    seen["link" .. m.d.modId] = true
                    add(m.d)
                    local ws = wsOf(m.d)
                    links[#links + 1] = self:mapInfo(f).title .. (ws and ws ~= "" and ws ~= "0" and (": https://steamcommunity.com/sharedfiles/filedetails/?id=" .. ws) or "")
                        .. "\nMod ID: " .. m.d.modId .. " - Map folder: " .. f
                end
            end
        end
    end
    table.sort(mods, function(a, b) return (self.place[a.modId] or 1e9) < (self.place[b.modId] or 1e9) end)
    local ids, items, sw = {}, {}, {}
    for _, d in ipairs(mods) do
        ids[#ids + 1] = d.modId
        local ws = wsOf(d)
        if ws and ws ~= "" and ws ~= "0" and not sw[ws] then
            sw[ws] = true
            items[#items + 1] = ws
        end
    end
    local lines = {
        mods = "Mods=" .. table.concat(ids, ";"),
        items = "WorkshopItems=" .. table.concat(items, ";"),
        map = "Map=" .. table.concat(order, ";"),
        links = table.concat(links, "\n"),
    }
    local out = ({
        mods = lines.mods, items = lines.items, map = lines.map, links = lines.links,
        all = lines.links .. "\n\n" .. lines.mods .. "\n" .. lines.items .. "\n" .. lines.map,
    })[kind]
    pcall(function() Clipboard.setClipboard(out) end)
    self:say("Copied: " .. (kind == "all" and "the maps with their Workshop links and the Mods=, WorkshopItems= and Map= lines." or kind == "links" and (#links .. " maps with their Workshop links.") or (lines[kind]:match("^[^=]+") .. "=")) .. " Paste it where you need it.")
end

--- Your hosted servers (Zomboid/Server/*.ini), read once per visit.
function PZMMMods:hostedList()
    if not self.hosted then
        self.hosted = {}
        pcall(function()
            local m = getServerSettingsManager()
            m:readAllSettings()
            for i = 0, m:getSettingsCount() - 1 do self.hosted[#self.hosted + 1] = m:getSettingsByIndex(i) end
        end)
    end
    return self.hosted
end

--- Several of a hosted server's options as text ("" without one), the files read once.
function PZMMMods:hostedOptions(st, names)
    local out = {}
    for _, n in ipairs(names) do out[n] = "" end
    pcall(function() st:loadFiles() end)
    for _, n in ipairs(names) do
        pcall(function() out[n] = tostring(st:getServerOptions():getOptionByName(n):getValue() or "") end)
    end
    return out
end

--- One of a hosted server's options as text ("" without it).
function PZMMMods:hostedOption(st, name)
    local v = ""
    pcall(function()
        st:loadFiles()
        v = tostring(st:getServerOptions():getOptionByName(name):getValue() or "")
    end)
    return v
end

--- Compare with a server: its Map= against your map order, as rows under the tools (drawCompare).
function PZMMMods:compareMaps(st)
    local name = ""
    pcall(function() name = st:getName() end)
    local theirs, mine = {}, self:maps()
    for f in self:hostedOption(st, "Map"):gmatch("[^;]+") do
        f = f:gsub("^%s+", ""):gsub("%s+$", "")
        if f ~= "" then theirs[#theirs + 1] = f end
    end
    local inT, inM = {}, {}
    for _, f in ipairs(theirs) do inT[f] = true end
    for _, f in ipairs(mine) do inM[f] = true end
    local s = self:mapScan()
    local onlyS, onlyY, a, b = {}, {}, {}, {}
    for _, f in ipairs(theirs) do
        if not inM[f] then
            local d
            for _, e in ipairs(s.entries) do if e.folder == f then d = e.d end end
            onlyS[#onlyS + 1] = { folder = f, d = d }
        else
            a[#a + 1] = f
        end
    end
    for _, f in ipairs(mine) do
        if not inT[f] then onlyY[#onlyY + 1] = f else b[#b + 1] = f end
    end
    local order
    if table.concat(a, ";") ~= table.concat(b, ";") then
        order = { "Order differs. Server: " .. table.concat(a, ", "), "Order differs. You: " .. table.concat(b, ", ") }
    end
    self.compared = { name = name, onlyServer = onlyS, onlyYou = onlyY, order = order }
end

--- The Compare result: what only the server has (with Enable, Preview, Workshop page), what only you have, the order.
function PZMMMods:drawCompare(x, y, w, mx, my)
    local c = self.compared
    local rh = BAR + 4
    local n = #c.onlyServer
    local lines = {}
    if #c.onlyYou > 0 then lines[#lines + 1] = "Only here: " .. table.concat(c.onlyYou, ", ") end
    for _, l in ipairs(c.order or {}) do lines[#lines + 1] = l end
    if n == 0 and #lines == 0 then lines[1] = "Same maps in the same order." end
    local h = 16 + H_MEDIUM + 10 + n * rh + #lines * (H_SMALL + 4) + 14
    card(self, x, y, w, h)
    bold(self, "Your maps and " .. c.name, x + 18, y + 16, C.text, MEDIUM)
    local cx = x + w - 16 - BAR
    local _, _, over = toolButton(self, TB.x, nil, cx, y + 8, mx, my)
    if over then tip(self, "Close") end
    self.feedHits[#self.feedHits + 1] = { kind = "fn", fn = function() self.compared = nil end, x = cx, y = y + 8, w = BAR, h = BAR }
    local ry = y + 16 + H_MEDIUM + 10
    if n > 0 then
        text(self, "Only on the server:", x + 18, ry - 8, C.muted)
        ry = ry + 8
    end
    for _, r in ipairs(c.onlyServer) do
        local room = self:drawRowActions(self:mapRowActions(r.d), x + w - 18, ry, rh, mx, my) - 10
        local ty = ry + math.floor((rh - H_SMALL) / 2)
        local hint = r.d and (r.d.name .. (r.d.isActive and " (installed, on)" or " (installed, off)")) or "not installed"
        bold(self, fit(r.folder, room - x - 18), x + 18, ty, C.text)
        local fx = x + 18 + width(r.folder) + 12
        if room - fx > 20 then text(self, fit(hint, room - fx), fx, ty, r.d and C.muted or C.warn) end
        ry = ry + rh
    end
    for _, l in ipairs(lines) do
        text(self, fit(l, w - 36), x + 18, ry + 2, C.muted)
        ry = ry + H_SMALL + 4
    end
    return y + h + 16
end

--- Map presets (Zomboid/Lua/pzmm/map_presets.txt: name, a tab, mod IDs with ; between): the map mods shown on the grid.
function PZMMMods:mapPresets()
    local out = {}
    pcall(function()
        local r = getFileReader("pzmm/map_presets.txt", false)
        if not r then return end
        local line = r:readLine()
        while line do
            local name, ids = line:match("^([^\t]+)\t(.*)$")
            if name then out[name] = ids end
            line = r:readLine()
        end
        r:close()
    end)
    return out
end
function PZMMMods:saveMapPresets(p)
    local names = {}
    for n in pairs(p) do names[#names + 1] = n end
    table.sort(names)
    local lines = {}
    for _, n in ipairs(names) do lines[#lines + 1] = n .. "\t" .. p[n] end
    pcall(function()
        local w = getFileWriter("pzmm/map_presets.txt", true, false)
        w:write(table.concat(lines, "\n") .. (#lines > 0 and "\n" or ""))
        w:close()
    end)
end
function PZMMMods:mapPresetMenu(mx, my)
    local s = self:mapScan()
    local p = self:mapPresets()
    local names = {}
    for n in pairs(p) do names[#names + 1] = n end
    table.sort(names, function(a, b) return a:lower() < b:lower() end)
    local items = {}
    for _, n in ipairs(names) do
        items[#items + 1] = { "Show " .. n, function()
            self:mapRemember()
            local set, count = {}, 0
            for id in p[n]:gmatch("[^;]+") do set[id] = true end
            for _, m in ipairs(s.mods) do
                for _, e in ipairs(m.entries) do self.mapShow[e.key] = set[m.d.modId] == true end
            end
            local gone = 0
            for id in pairs(set) do
                count = count + 1
                if not s.modOf[id] then gone = gone + 1 end
            end
            self.showVer = (self.showVer or 0) + 1
            self.gvFit = true
            self:say(n .. " shown on the grid" .. (gone > 0 and (": " .. gone .. " of its maps aren't installed.") or "."))
        end }
    end
    if #items > 0 then items[#items + 1] = false end
    items[#items + 1] = { "Save the shown maps as a preset", function()
        self:prompt("Save the shown maps as", "", "Save", function(name)
            name = (name or ""):gsub("^%s+", ""):gsub("%s+$", ""):gsub("[\t\r\n]", " ")
            if name == "" then return end
            local ids = {}
            for _, m in ipairs(s.mods) do
                for _, e in ipairs(m.entries) do
                    if self:mapShown(e) then
                        ids[#ids + 1] = m.d.modId
                        break
                    end
                end
            end
            local all = self:mapPresets()
            local function save()
                local cur = self:mapPresets()
                cur[name] = table.concat(ids, ";")
                self:saveMapPresets(cur)
                self:say(name .. " saved: " .. #ids .. " shown maps.")
            end
            if all[name] ~= nil then
                self:ask({ title = T("maps_replace_preset", "Replace the preset \"%1\"?", name), choices = { { T("maps_replace", "Replace"), save } } })
            else
                save()
            end
        end)
    end }
    if #names > 0 then
        items[#items + 1] = { "Delete a preset...", function()
            local sub = {}
            for _, n in ipairs(names) do
                sub[#sub + 1] = { n, function()
                    local all = self:mapPresets()
                    all[n] = nil
                    self:saveMapPresets(all)
                    self:say(n .. " deleted.")
                end, danger = true }
            end
            self:menu(sub, mx, my)
        end, danger = true }
    end
    self:menu(items, mx, my)
end

--- "Map folder X is also added by Y": two installed mods adding the same map folder. modId -> { lines } (cached per scan state).
function PZMMMods:mapDupes(d)
    local sc = self:mapScan()
    local c = self.dupeCache
    if not c or c.ver ~= sc.ver then
        local byFolder = {}
        for _, e in ipairs(sc.entries) do
            local k = e.folder:lower()
            byFolder[k] = byFolder[k] or {}
            table.insert(byFolder[k], e)
        end
        c = { ver = sc.ver, by = {} }
        for _, list in pairs(byFolder) do
            if #list > 1 then
                for _, e in ipairs(list) do
                    for _, o in ipairs(list) do
                        if o.d ~= e.d then
                            local t = c.by[e.d.modId] or {}
                            c.by[e.d.modId] = t
                            t[#t + 1] = "Map folder " .. e.folder .. " is also added by " .. (o.d.name or o.d.modId)
                        end
                    end
                end
            end
        end
        for _, t in pairs(c.by) do table.sort(t) end
        self.dupeCache = c
    end
    return c.by[d.modId] or {}
end

--- The strip for the picked map mod: its picture and description (from its map.info), and what to do with it.
function PZMMMods:drawMapStrip(x, y, w, mx, my)
    local m = self.mapSel
    local d = m.d
    local e1 = m.entries[1]
    local info = self:mapInfo(e1.folder)
    if info.tex == nil then
        info.tex = false
        pcall(function() if info.thumb and info.thumb ~= "" then info.tex = getTexture(info.thumb) or false end end)
    end
    local tw = info.tex and 200 or 0
    local th = info.tex and math.min(130, math.floor(tw * info.tex:getHeight() / math.max(1, info.tex:getWidth()))) or 0
    local tx = x + 18 + (info.tex and tw + 18 or 0)
    local room = x + w - 18 - tx
    if info.descW ~= room then
        info.descLines = wrap((tostring(info.desc or ""):gsub("<[^>]*>", " ")), room)
        info.descW = room
    end
    local folders = {}
    for _, e in ipairs(m.entries) do folders[#folders + 1] = e.folder end
    local n = 0
    for _, e in ipairs(m.entries) do n = n + #e.cells end
    local nd = math.min(#info.descLines, 5)
    local dupes = self:mapDupes(d)
    local conf = self:mapConflictCells(m)
    local nconf = math.min(#conf, 3)
    local pack = self:packSize(d)
    local author = ""
    pcall(function() author = tostring(d.modInfo:getAuthor() or "") end)
    local stp = self:steamOf(d)
    local h = 18 + H_MEDIUM + 4 + (H_SMALL + 4) * (2 + math.min(#dupes, 3) + (pack > 1 and 1 or 0)) + (nconf > 0 and (BAR + 2 + (nconf - 1) * (H_SMALL + 4)) or 0) + nd * (H_SMALL + 2) + 10 + BAR + 16
    h = math.max(h, th + 36)
    card(self, x, y, w, h)
    if info.tex then
        rect(self, x + 18, y + 18, tw, th, C.raised, 1)
        self:drawTextureScaledAspect(info.tex, x + 18, y + 18, tw, th, 1, 1, 1, 1)
    end
    local ty = y + 18
    bold(self, fit(info.title, room), tx, ty, C.text, MEDIUM)
    ty = ty + H_MEDIUM + 4
    text(self, fit(d.name .. "   " .. d.modId .. (author ~= "" and ("   by " .. author) or "") .. ((stp and (stp.updated or 0) > 0) and ("   updated " .. K.ago(stp.updated)) or ""), room), tx, ty, C.muted)
    ty = ty + H_SMALL + 4
    if pack > 1 then
        text(self, fit(T("maps_pack", "1 of %1 mods in this Workshop item", pack), room), tx, ty, C.muted)
        ty = ty + H_SMALL + 4
    end
    text(self, fit("Folders: " .. table.concat(folders, ", ") .. (e1.done and ("    " .. n .. " cells") or "    counting cells..."), room), tx, ty, C.muted)
    ty = ty + H_SMALL + 4
    for i = 1, math.min(#dupes, 3) do
        text(self, fit(dupes[i], room), tx, ty, C.warn)
        ty = ty + H_SMALL + 4
    end
    -- where it clashes, with Swap inline
    for i = 1, nconf do
        local line = "Conflicts with " .. (conf[i].d.name or conf[i].d.modId) .. " on cells " .. conf[i].text
        if i == 1 then
            local sl = T("maps_swap", "Swap")
            local sw = width(sl) + 28
            local sx = x + w - 18 - sw
            local _, _, so = flatButton(self, sl, sx, ty, mx, my, "quiet")
            if so then tip(self, "Swap\nShow this map instead: hide the previewed maps it conflicts with, turn off the enabled ones.") end
            self.feedHits[#self.feedHits + 1] = { kind = "fn", fn = function() self:mapSwap(m) end, x = sx, y = ty, w = sw, h = BAR }
            text(self, fit(line, sx - 12 - tx), tx, ty + math.floor((BAR - H_SMALL) / 2), C.danger)
            ty = ty + BAR + 2
        else
            text(self, fit(line, room), tx, ty, C.danger)
            ty = ty + H_SMALL + 4
        end
    end
    for i = 1, nd do
        text(self, info.descLines[i], tx, ty, C.desc)
        ty = ty + H_SMALL + 2
    end
    ty = math.max(ty + 10, y + h - 16 - BAR)
    local ws = wsOf(d)
    local shown = false
    for _, e in ipairs(m.entries) do if self:mapShown(e) then shown = true end end
    local acts = {
        { "Show in My mods", function() self.page, self.selected, self.tab, self.modsText = "mods", d, "All", ""; self:updateView() end },
        { shown and "Hide from the grid" or "Show on the grid", function() self:mapToggleShow(m) end },
    }
    if ws and ws ~= "" and ws ~= "0" then acts[#acts + 1] = { "Workshop page", function() self:openWorkshop(ws) end } end
    acts[#acts + 1] = { "Close", function() self.mapSel = nil end }
    local bx = tx
    for _, a in ipairs(acts) do
        local bw = width(a[1]) + 28
        flatButton(self, a[1], bx, ty, mx, my, "quiet")
        self.feedHits[#self.feedHits + 1] = { kind = "fn", fn = a[2], x = bx, y = ty, w = bw, h = BAR }
        bx = bx + bw + 6
    end
    return y + h + 16
end

--- Maps' own undo: the map order, which map mods are on and what's previewed, before each change on the page.
function PZMMMods:mapSnap()
    local active, show, order = {}, {}, {}
    for _, m in ipairs(self:mapScan().mods) do active[m.d.modId] = m.d.isActive and true or false end
    for k, v in pairs(self.mapShow or {}) do show[k] = v end
    for i, m in ipairs(self:maps()) do order[i] = m end
    return { active = active, show = show, order = order }
end
function PZMMMods:mapRemember()
    self.mapUndos = self.mapUndos or {}
    table.insert(self.mapUndos, self:mapSnap())
    while #self.mapUndos > 50 do table.remove(self.mapUndos, 1) end
end
function PZMMMods:mapUndo()
    local u = self.mapUndos and table.remove(self.mapUndos)
    if not u then return end
    local ons, offs = {}, {}
    for _, m in ipairs(self:mapScan().mods) do
        local want = u.active[m.d.modId]
        if want ~= nil and want ~= (m.d.isActive and true or false) then
            local l = want and ons or offs
            l[#l + 1] = m.d
        end
    end
    self.mapShow = u.show
    self.showVer = (self.showVer or 0) + 1
    if #ons + #offs > 0 then self:setMany(ons, offs) end
    self.mapOrder = baseLast(u.order)
    self:setMapOrder(self.mapOrder)
    self.diffAt = nil
    self:say("Undone.")
end

--- The other maps (enabled or on the grid) that share cells with map mod `m`: modId -> cells. Only once the scan is done;
--- an add-on over its base isn't a clash.
function PZMMMods:mapConflictsOf(m)
    local s = self:mapScan()
    if not s.finished then return {} end
    local key = s.ver .. "|" .. (self.modVer or 0) .. "|" .. (self.showVer or 0)
    if self.confKey ~= key then self.confCache, self.confKey = {}, key end
    local c = self.confCache[m.d.modId]
    if c then return c end
    c = {}
    for _, e in ipairs(m.entries) do
        for _, k in ipairs(e.cells) do
            local o = s.owners[k]
            if o and #o > 1 then
                for _, e2 in ipairs(o) do
                    if e2.d ~= m.d and (e2.d.isActive or self:mapShown(e2)) and not needsMod(m.d, e2.d) and not needsMod(e2.d, m.d) then
                        c[e2.d.modId] = (c[e2.d.modId] or 0) + 1
                    end
                end
            end
        end
    end
    self.confCache[m.d.modId] = c
    return c
end

--- Swap: show map mod `m` instead of the maps it clashes with (the previewed ones come off the grid, the enabled ones turn off).
function PZMMMods:mapSwap(m)
    local s = self:mapScan()
    self:mapRemember()
    local snap = self:snapshot()
    local offs, hid = {}, 0
    for id in pairs(self:mapConflictsOf(m)) do
        local d = self.model.mods[id]
        local mm = s.modOf[id]
        if d and d.isActive then offs[#offs + 1] = d else hid = hid + 1 end
        if mm then for _, e in ipairs(mm.entries) do self.mapShow[e.key] = false end end
    end
    for _, e in ipairs(m.entries) do self.mapShow[e.key] = true end
    self.showVer = (self.showVer or 0) + 1
    if #offs > 0 then self:setMany({}, offs) end
    self:rememberIfChanged(snap)
    self:say("Showing " .. m.d.name .. " instead: " .. #offs .. " turned off, " .. hid .. " taken off the grid.")
end

--- Most maps that fit: preview the most listed maps (not enabled) that share no cell with each other or the enabled ones.
--- Greedy, smallest first, so the most maps fit.
function PZMMMods:mapQuickFit()
    local s = self:mapScan()
    if not s.finished then return self:say("Still reading the map files: try again in a moment.") end
    self:mapRemember()
    local taken = {}
    for _, m in ipairs(s.mods) do
        if m.d.isActive then
            for _, e in ipairs(m.entries) do for _, k in ipairs(e.cells) do taken[k] = true end end
        end
    end
    local cands = {}
    for _, m in ipairs(s.mods) do
        if not m.d.isActive then
            local n = 0
            for _, e in ipairs(m.entries) do
                n = n + #e.cells
                self.mapShow[e.key] = false
            end
            if n > 0 then cands[#cands + 1] = { m = m, n = n } end
        end
    end
    table.sort(cands, function(a, b)
        if a.n ~= b.n then return a.n < b.n end
        return byName(a.m.d, b.m.d)
    end)
    local picked = 0
    for _, c in ipairs(cands) do
        local clash = false
        for _, e in ipairs(c.m.entries) do
            for _, k in ipairs(e.cells) do
                if taken[k] then clash = true; break end
            end
            if clash then break end
        end
        if not clash then
            picked = picked + 1
            for _, e in ipairs(c.m.entries) do
                for _, k in ipairs(e.cells) do taken[k] = true end
                self.mapShow[e.key] = true
            end
        end
    end
    self.showVer = (self.showVer or 0) + 1
    self.gvFit = true
    self:say(picked > 0 and ("Previewing " .. picked .. (picked == 1 and " map" or " maps") .. ": the most of the listed maps that fit together around the enabled ones.") or "Every listed map shares cells with an enabled one.")
end

--- Preview every listed map you haven't enabled (or take them all off the grid).
function PZMMMods:mapPreviewListed(on)
    self:mapRemember()
    for _, m in ipairs(self:mapRows()) do
        if not m.d.isActive then
            for _, e in ipairs(m.entries) do self.mapShow[e.key] = on end
        end
    end
    self.showVer = (self.showVer or 0) + 1
end

--- What map mod `d` needs: the mods it requires and the maps it builds on (lots=), as ids with the map folder when it
--- is a map. Returns the list of its direct needs and which of them are maps.
function PZMMMods:mapNeedsOf(mod)
    local s = self:mapScan()
    local ids, via = {}, {}
    for id in pairs(mod.requireMods or {}) do ids[id] = true end
    for _, q in ipairs(rules(mod).requires) do ids[q] = true end
    local m = s.modOf[mod.modId]
    if m then
        for _, e in ipairs(m.entries) do
            for _, l in ipairs(self:mapInfo(e.folder).lots) do
                for _, o in ipairs(s.entries) do
                    if o.folder == l and o.d ~= mod then
                        ids[o.d.modId] = true
                        via[o.d.modId] = l
                    end
                end
            end
        end
    end
    local arr = {}
    for id in pairs(ids) do arr[#arr + 1] = id end
    table.sort(arr)
    return arr, via
end

--- What can be done to a map mod from a Requirements or Compare row: Enable, Preview (on the grid), Workshop page.
function PZMMMods:mapRowActions(d)
    local out = {}
    if not d then return out end
    local m = self:mapScan().modOf[d.modId]
    if not d.isActive and d.isAvailable ~= false then
        out[#out + 1] = { "Enable", "Enable\nTurns it on (and what it needs).", function() self:mapRemember(); self:toggle(d) end, TB.check }
    end
    if m and not d.isActive then
        local shown = false
        for _, e in ipairs(m.entries) do if self:mapShown(e) then shown = true end end
        out[#out + 1] = { shown and "Unpreview" or "Preview", shown and "Unpreview\nTake it off the grid." or "Preview\nShow it on the grid without enabling it.", function() self:mapToggleShow(m) end, TB.search }
    end
    local ws = wsOf(d)
    if ws and ws ~= "" and ws ~= "0" then
        out[#out + 1] = { "Workshop page", "Workshop page\nOpens it in Steam.", function() self:openWorkshop(ws) end, TB.steam }
    end
    return out
end
--- Draw those buttons right to left, the first at `right`, in a row at `y` of height `rh`; returns the left edge.
function PZMMMods:drawRowActions(btns, right, y, rh, mx, my)
    for _, b in ipairs(btns) do
        local bw = width(b[1]) + 20 + 28
        right = right - bw
        local by = y + math.floor((rh - BAR) / 2)
        local _, _, over = flatButton(self, b[1], right, by, mx, my, nil, b[4])
        if over then tip(self, b[2]) end
        self.feedHits[#self.feedHits + 1] = { kind = "fn", fn = b[3], x = right, y = by, w = bw, h = BAR }
        right = right - 6
    end
    return right
end

--- The picked map mod's requirements, as the app's Requirements view: "Needed by" (each requirement and who needs it)
--- or "Tree" (what it needs, all the way down). Returns the y under it.
function PZMMMods:drawMapReqs(x, y, w, mx, my)
    local sel = self.mapSel.d
    local view = self.reqView or "needed"
    -- worked out again only when the mods or the pick change
    local key = tostring(sel.modId) .. "|" .. (self.modVer or 0) .. "|" .. view .. "|" .. (self.mapScanState and self.mapScanState.ver or 0)
    if not self.reqCache or self.reqCache.key ~= key then
        local lines = {}
        if view == "tree" then
            local function walk(id, depth, path, via)
                if #lines >= 80 then return end
                local mod = self.model.mods[id]
                lines[#lines + 1] = { id = id, d = mod, depth = depth, via = via, loop = path[id] }
                if path[id] or not mod then return end
                path[id] = true
                local arr, vias = self:mapNeedsOf(mod)
                for _, q in ipairs(arr) do walk(q, depth + 1, path, vias[q]) end
                path[id] = nil
            end
            local arr, vias = self:mapNeedsOf(sel)
            for _, q in ipairs(arr) do walk(q, 0, { [sel.modId] = true }, vias[q]) end
        else
            local byReq, order, seen, todo = {}, {}, { [sel.modId] = true }, { sel }
            while #todo > 0 and #order < 80 do
                local mod = table.remove(todo, 1)
                local arr, vias = self:mapNeedsOf(mod)
                for _, q in ipairs(arr) do
                    if not byReq[q] then
                        byReq[q] = { names = {}, via = vias[q] }
                        order[#order + 1] = q
                    end
                    table.insert(byReq[q].names, mod.name or mod.modId)
                    local qd = self.model.mods[q]
                    if qd and not seen[q] then
                        seen[q] = true
                        todo[#todo + 1] = qd
                    end
                end
            end
            table.sort(order, function(a, b)
                local da, db = self.model.mods[a], self.model.mods[b]
                return ((da and da.name) or a):lower() < ((db and db.name) or b):lower()
            end)
            for _, q in ipairs(order) do
                lines[#lines + 1] = { id = q, d = self.model.mods[q], depth = 0, via = byReq[q].via, by = table.concat(byReq[q].names, ", ") }
            end
        end
        self.reqCache = { key = key, lines = lines }
    end
    local lines = self.reqCache.lines
    local rh = BAR + 4
    local h = 18 + BAR + 10 + math.max(1, #lines) * rh + 14
    card(self, x, y, w, h)
    bold(self, T("maps_req_title", "Requirements"), x + 18, y + 18 + (BAR - H_SMALL) / 2, C.text)
    -- Needed by | Tree, one segmented control at the right
    local tabs = { { "needed", T("maps_req_needed", "Needed by") }, { "tree", T("maps_req_tree", "Tree") } }
    local sw = 0
    for _, t in ipairs(tabs) do sw = sw + width(t[2]) + 24 end
    local tx = x + w - 18 - sw
    local ty = y + 18
    for i, t in ipairs(tabs) do
        local tw = width(t[2]) + 24
        local on = view == t[1]
        if on then roundEnds(self, tx, ty, tw, BAR, 6, C.accent, 0.32, i == 1, i == #tabs) end
        local over = mx >= tx and mx < tx + tw and my >= ty and my < ty + BAR
        local f = on and bold or text
        f(self, t[2], tx + 12, ty + (BAR - H_SMALL) / 2, on and C.onSelected or over and C.text or C.muted)
        local id = t[1]
        self.feedHits[#self.feedHits + 1] = { kind = "fn", fn = function() self.reqView = id end, x = tx, y = ty, w = tw, h = BAR }
        tx = tx + tw
    end
    outline(self, x + w - 18 - sw, ty, sw, BAR, 6, C.line)
    local ry = ty + BAR + 10
    if #lines == 0 then
        text(self, "The picked map needs nothing else.", x + 18, ry + 4, C.muted)
    end
    for _, l in ipairs(lines) do
        local d = l.d
        local lx = x + 18 + l.depth * 18
        local room = self:drawRowActions(self:mapRowActions(d), x + w - 18, ry, rh, mx, my) - 10
        local name = fit(d and d.name or l.id, room - lx - 120)
        local ty = ry + math.floor((rh - H_SMALL) / 2)
        bold(self, name, lx, ty, d and C.text or C.warn)
        lx = lx + width(name) + 10
        local st = not d and "Not installed" or d.isActive and "On" or "Off"
        lx = chip(self, st, lx, ty - 1, not d and C.warn or d.isActive and C.ok or nil) + 10
        local extra = l.loop and "(needs itself: a loop)" or l.by and ("needed by " .. l.by) or nil
        if l.via then extra = "map " .. l.via .. (extra and (" - " .. extra) or "") end
        if extra and room - lx > 20 then text(self, fit(extra, room - lx), lx, ty, C.muted) end
        ry = ry + rh
    end
    return y + h + 16
end


--- The requirements of every shown map (enabled or previewed), as the app's Requirements: grouped by what needs them,
--- with how many aren't installed. Folded when nothing is missing. Returns the y under it.
function PZMMMods:drawMapReqsAll(x, y, w, mx, my)
    local s = self:mapScan()
    local key = (self.modVer or 0) .. "|" .. (self.showVer or 0) .. "|" .. s.ver
    local c = self.reqAllCache
    if not c or c.key ~= key then
        local groups = { enabled = { order = {}, by = {} }, preview = { order = {}, by = {} } }
        local missing, seenMissing, shownN = 0, {}, 0
        for _, m in ipairs(s.mods) do
            local on, pv = m.d.isActive, false
            if not on then
                for _, e in ipairs(m.entries) do if self:mapShown(e) then pv = true end end
            end
            if on or pv then
                shownN = shownN + 1
                local g = groups[on and "enabled" or "preview"]
                -- everything it needs, directly or through what those need
                local seen, todo, nn = { [m.d.modId] = true }, { m.d }, 0
                while #todo > 0 and nn < 60 do
                    local mod = table.remove(todo, 1)
                    for _, q in ipairs((self:mapNeedsOf(mod))) do
                        if not seen[q] then
                            seen[q] = true
                            nn = nn + 1
                            if not g.by[q] then g.by[q] = {}; g.order[#g.order + 1] = q end
                            table.insert(g.by[q], m.d.name or m.d.modId)
                            local qd = self.model.mods[q]
                            if qd then
                                todo[#todo + 1] = qd
                            elseif not seenMissing[q] then
                                seenMissing[q] = true
                                missing = missing + 1
                            end
                        end
                    end
                end
            end
        end
        for _, g in pairs(groups) do table.sort(g.order) end
        c = { key = key, groups = groups, missing = missing, shown = shownN }
        self.reqAllCache = c
    end
    if c.shown == 0 then return y end
    local open = self.reqAllOpen
    if open == nil then open = c.missing > 0 end
    local rh = BAR + 4
    local lines, total = {}, 0
    for _, gid in ipairs({ "enabled", "preview" }) do
        local g = c.groups[gid]
        if #g.order > 0 then
            lines[#lines + 1] = { head = gid == "enabled" and T("maps_req_g_enabled", "Needed by enabled maps") or T("maps_req_g_preview", "Needed by previewed maps") }
            for _, q in ipairs(g.order) do
                if total < 40 then lines[#lines + 1] = { id = q, by = table.concat(g.by[q], ", ") } end
                total = total + 1
            end
        end
    end
    local hh = 18 + BAR + 14
    local h = hh
    if open then
        h = hh + (#lines == 0 and (H_SMALL + 8) or 0) + #lines * rh + (total > 40 and (H_SMALL + 8) or 0)
    end
    card(self, x, y, w, h)
    local chev = open and K.CHEV_DOWN or K.CHEV_RIGHT
    if chev then self:drawTextureScaled(chev, x + 16, y + 18 + math.floor((BAR - 14) / 2), 14, 14, 1, C.muted.r, C.muted.g, C.muted.b) end
    local title = T("maps_req_title", "Requirements")
    bold(self, title, x + 36, y + 18 + (BAR - H_SMALL) / 2, C.text)
    if c.missing > 0 then
        chip(self, T("maps_req_missing", "%1 missing", c.missing), x + 36 + width(title) + 12, y + 18 + math.floor((BAR - H_SMALL - 2) / 2), C.warn)
    end
    self.feedHits[#self.feedHits + 1] = { kind = "fn", fn = function() self.reqAllOpen = not open end, x = x, y = y, w = w, h = hh }
    if not open then return y + h + 16 end
    local ry = y + 18 + BAR + 6
    if #lines == 0 then text(self, T("maps_req_none", "The shown maps need nothing else."), x + 18, ry, C.muted) end
    for _, l in ipairs(lines) do
        if l.head then
            bold(self, l.head, x + 18, ry + math.floor((rh - H_SMALL) / 2), C.muted)
        else
            local d = self.model.mods[l.id]
            local room = self:drawRowActions(self:mapRowActions(d), x + w - 18, ry, rh, mx, my) - 10
            local lx = x + 18
            local name = fit(d and d.name or l.id, room - lx - 120)
            local ty = ry + math.floor((rh - H_SMALL) / 2)
            bold(self, name, lx, ty, d and C.text or C.warn)
            lx = lx + width(name) + 10
            lx = chip(self, not d and T("maps_not_installed", "Not installed") or d.isActive and T("maps_req_on", "On") or T("maps_req_off", "Off"), lx, ty - 1, not d and C.warn or d.isActive and C.ok or nil) + 10
            if room - lx > 20 then text(self, fit("needed by " .. l.by, room - lx), lx, ty, C.muted) end
        end
        ry = ry + rh
    end
    if total > 40 then text(self, "+" .. (total - 40) .. " more", x + 18, ry, C.muted) end
    return y + h + 16
end

--- Where a map mod shares cells with others that are shown: { { d = the other mod, text = "x,y | x,y +N" }, ... }.
function PZMMMods:mapConflictCells(m)
    local sc = self:mapScan()
    if not sc.finished then return {} end
    local key = sc.ver .. "|" .. (self.modVer or 0) .. "|" .. (self.showVer or 0)
    local cc = self.ccCache
    if not cc or cc.key ~= key then cc = { key = key, by = {} }; self.ccCache = cc end
    local r = cc.by[m.d.modId]
    if r then return r end
    local per, order = {}, {}
    for _, e in ipairs(m.entries) do
        for _, k in ipairs(e.cells) do
            local o = sc.owners[k]
            if o and #o > 1 then
                for _, e2 in ipairs(o) do
                    if e2.d ~= m.d and (e2.d.isActive or self:mapShown(e2)) and not needsMod(m.d, e2.d) and not needsMod(e2.d, m.d) then
                        local pe = per[e2.d.modId]
                        if not pe then pe = { d = e2.d, cells = {} }; per[e2.d.modId] = pe; order[#order + 1] = pe end
                        pe.cells[#pe.cells + 1] = k
                    end
                end
            end
        end
    end
    r = {}
    for _, pe in ipairs(order) do
        local parts = {}
        for i = 1, math.min(3, #pe.cells) do
            local k = pe.cells[i]
            parts[i] = (math.floor(k / 10000) - 500) .. "," .. (k % 10000 - 500)
        end
        r[#r + 1] = { d = pe.d, text = table.concat(parts, " | ") .. (#pe.cells > 3 and (" +" .. (#pe.cells - 3)) or "") }
    end
    table.sort(r, function(a, b) return (a.d.name or a.d.modId):lower() < (b.d.name or b.d.modId):lower() end)
    cc.by[m.d.modId] = r
    return r
end

--- How many installed mods share this mod's Workshop item (more than one: a mod pack).
function PZMMMods:packSize(d)
    local ws = wsOf(d)
    if ws == "" then return 1 end
    local c = self.packC
    if not c or c.v ~= self.modVer then
        c = { v = self.modVer, n = {} }
        for _, o in pairs(self.model.mods) do
            local w = wsOf(o)
            if w ~= "" then c.n[w] = (c.n[w] or 0) + 1 end
        end
        self.packC = c
    end
    return c.n[ws] or 1
end

--- Show a map mod's cells on the grid, or hide them.
function PZMMMods:mapToggleShow(m)
    local shown = false
    for _, e in ipairs(m.entries) do if self:mapShown(e) then shown = true end end
    self:mapRemember()
    for _, e in ipairs(m.entries) do self.mapShow[e.key] = not shown end
    self.showVer = (self.showVer or 0) + 1
end

--- Zoom the grid by `f` (2 to 96 px a cell, whole numbers) about the point mx, my (default: its centre).
function PZMMMods:zoomGrid(f, mx, my)
    local gb, gv = self.gridBox, self.gv
    if not (gb and gv) then return end
    mx, my = mx or (gb.x + gb.w / 2), my or (gb.y + gb.h / 2)
    local z = gv.zoom
    local wx, wy = (mx - gb.ox) / z, (my - gb.oy) / z
    local nz = math.floor(z * f + 0.5)
    if nz == z then nz = z + (f > 1 and 1 or -1) end
    nz = math.max(2, math.min(96, nz))
    gv.zoom = nz
    gv.cx = wx - (mx - (gb.x + gb.w / 2)) / nz
    gv.cy = wy - (my - (gb.y + gb.h / 2)) / nz
    self.gvFit = false
end

--- A click on the grid (not a drag) picks the cell under it; Shift adds or drops cells. The map list then
--- shows only the maps at the picked cells.
function PZMMMods:pickCell(mx, my)
    local gb, gv = self.gridBox, self.gv
    if not (gb and gv) then return end
    local hx, hy = math.floor((mx - gb.ox) / gv.zoom), math.floor((my - gb.oy) / gv.zoom)
    if hx <= -499 or hx >= 9499 or hy <= -499 or hy >= 9499 then return end
    local k = (hx + 500) * 10000 + (hy + 500)
    local shift = false
    pcall(function() shift = isShiftKeyDown() end)
    local sel = self.gridSel or { keys = {}, set = {} }
    if not shift and #sel.keys == 1 and sel.set[k] then
        sel = { keys = {}, set = {} }
    elseif not shift then
        sel = { keys = { k }, set = { [k] = true } }
    elseif sel.set[k] then
        sel.set[k] = nil
        for i, v in ipairs(sel.keys) do if v == k then table.remove(sel.keys, i) break end end
    else
        sel.keys[#sel.keys + 1] = k
        sel.set[k] = true
    end
    self.gridSel = #sel.keys > 0 and sel or nil
    self.cellVer = (self.cellVer or 0) + 1
end

--- The cell grid: every map's cells in its colour (overlaps in red), zoomed with the wheel, panned by dragging;
--- the legend at its right toggles maps. Returns the y under it.
--- Knox County, the game's own map, as Build 42 cells (42/media/pzmm/vanilla_cells.txt, from its lotheaders:
--- "y x0-x1,..." a row a line): { runs = { { y, x0, x1 } }, set = { key = true }, x0, x1, y0, y1 }. Read once.
function K.vanilla()
    if K.VAN ~= nil then return K.VAN or nil end
    K.VAN = false
    pcall(function()
        local r = getModFileReader("PZMMClient", "media/pzmm/vanilla_cells.txt", false)
        if not r then return end
        local v = { runs = {}, set = {}, x0 = 1e9, x1 = -1e9, y0 = 1e9, y1 = -1e9 }
        local line = r:readLine()
        while line do
            local y, rest = line:match("^(%-?%d+) (.+)$")
            if y then
                y = tonumber(y)
                for a, b in rest:gmatch("(%-?%d+)%-(%-?%d+)") do
                    a, b = tonumber(a), tonumber(b)
                    v.runs[#v.runs + 1] = { y, a, b }
                    for cx = a, b do v.set[(cx + 500) * 10000 + (y + 500)] = true end
                    v.x0, v.x1 = math.min(v.x0, a), math.max(v.x1, b)
                    v.y0, v.y1 = math.min(v.y0, y), math.max(v.y1, y)
                end
            end
            line = r:readLine()
        end
        r:close()
        if #v.runs > 0 then K.VAN = v end
    end)
    return K.VAN or nil
end

function PZMMMods:drawGrid(x, y, w, mx, my)
    local s = self:mapScan()
    local g = self:gridCells()
    local clashes = self:mapClashes()
    local nclash = 0
    for _, m in ipairs(s.mods) do
        if m.d.isActive and clashes[m.d.modId] then
            local any = false
            for _ in pairs(clashes[m.d.modId].others) do any = true end
            for _ in pairs(clashes[m.d.modId].extra) do any = true end
            if any then nclash = nclash + 1 end
        end
    end
    local major = 42
    pcall(function() major = getCore():getGameVersion():getMajor() end)
    -- Knox County under the maps (its cells are Build 42's), unless hidden
    local V = major >= 42 and not self.hideVanilla and K.vanilla() or nil
    bold(self, T("grid_title", "Cell grid"), x, y, C.text, MEDIUM)
    local sub = g.shown .. (g.shown == 1 and " map" or " maps") .. " shown, " .. g.over .. " cells where they overlap, " .. nclash .. (nclash == 1 and " map" or " maps") .. " clashing"
    if not s.finished then sub = sub .. "    scanning " .. math.min(s.qi - 1, #s.queue) .. " of " .. #s.queue .. "..." end
    text(self, fit(sub, w - 640), x + width(T("grid_title", "Cell grid"), MEDIUM) + 16, y + (H_MEDIUM - H_SMALL) / 2, C.muted)
    -- Fit view, Zoom in, Zoom out and Per-map colours, right-aligned
    local by, right = y + (H_MEDIUM - BAR) / 2, x + w
    for _, b in ipairs({
        { T("maps_fit", "Fit"), "Fit\nZooms to every shown cell. The wheel zooms, dragging pans.", function() self.gvFit = true end },
        { "Zoom in", "Zoom in\nCloser, about the middle of the grid. The wheel zooms about the mouse.", function() self:zoomGrid(1.25) end },
        { "Zoom out", "Zoom out\nFarther away.", function() self:zoomGrid(0.8) end },
        { "Per-map colours", "Per-map colours\nEach map in its own colour, instead of one colour for every map. Overlaps stay red.", function() self.mapColours = not self.mapColours end, self.mapColours },
        { "Knox County", "Knox County\nThe game's own map in grey, a mod's cells over it in yellow. Click to hide or show it.", function() self.hideVanilla = not self.hideVanilla; self.gvFit = true end, major >= 42 and not self.hideVanilla },
    }) do
        local bw = width(b[1]) + 28
        local _, _, o = flatButton(self, b[1], right - bw, by, mx, my, b[4] and "primary" or "quiet")
        if o then tip(self, b[2]) end
        self.feedHits[#self.feedHits + 1] = { kind = "fn", fn = b[3], x = right - bw, y = by, w = bw, h = BAR }
        right = right - bw - 6
    end
    y = y + H_MEDIUM + 10
    local gh = 420
    card(self, x, y, w, gh)
    local lgw = math.min(280, math.floor(w * 0.3))
    local ix, iy, iw, ih = x + 12, y + 12, w - 36 - lgw, gh - 24
    rect(self, ix, iy, iw, ih, C.bg, 1)
    local gv = self.gv
    if not gv then
        gv = { cx = 30, cy = 30, zoom = 8 }
        self.gv, self.gvFit = gv, true
    end
    if self.gvFit and (g.n > 0 or V) then
        local x0, x1, y0, y1 = g.x0, g.x1, g.y0, g.y1
        if V then x0, x1, y0, y1 = math.min(x0, V.x0), math.max(x1, V.x1), math.min(y0, V.y0), math.max(y1, V.y1) end
        local bw, bh = x1 - x0 + 3, y1 - y0 + 3
        gv.zoom = math.max(2, math.min(96, math.floor(math.min(iw / bw, ih / bh))))
        gv.cx, gv.cy = (x0 + x1 + 1) / 2, (y0 + y1 + 1) / 2
    end
    local rmx, rmy = self:getMouseX(), self:getMouseY()
    local gd = self.gridDrag
    if gd then
        gv.cx = gd.cx - (rmx - gd.x0) / gv.zoom
        gv.cy = gd.cy - (rmy - gd.y0) / gv.zoom
        self.gvFit = false
    end
    local z = gv.zoom
    local ox, oy = ix + iw / 2 - gv.cx * z, iy + ih / 2 - gv.cy * z
    self.gridBox = { x = ix, y = iy, w = iw, h = ih, ox = ox, oy = oy }
    -- clipped to the grid and to the page's frame
    local fx, fy, fw, fh = self:feedBox()
    if self.page == "home" then fy, fh = TOP, self.height - BOTTOM - TOP end
    local cy0, cy1 = math.max(iy, fy), math.min(iy + ih, fy + fh)
    self.gridHit = { x = ix, y = cy0, w = iw, h = math.max(0, cy1 - cy0) }
    -- a 2 px ring round a cell, 1 px outside it
    local function ring(px, py, c)
        rect(self, px - 1, py - 1, z + 2, 2, c, 1)
        rect(self, px - 1, py + z - 1, z + 2, 2, c, 1)
        rect(self, px - 1, py + 1, 2, z - 2, c, 1)
        rect(self, px + z - 1, py + 1, 2, z - 2, c, 1)
    end
    if cy1 > cy0 then
        -- (clips nest: the page's comes off first, or one is left on and hides the footer)
        self:clearStencilRect()
        self:setStencilRect(ix, cy0, iw, cy1 - cy0)
        local step = 1
        for _, st in ipairs({ 1, 2, 5, 10, 20, 50, 100 }) do
            step = st
            if st * z >= 44 then break end
        end
        local c0, c1 = math.floor((ix - ox) / z), math.ceil((ix + iw - ox) / z)
        local r0, r1 = math.floor((iy - oy) / z), math.ceil((iy + ih - oy) / z)
        for cx = c0 - c0 % step, c1, step do rect(self, math.floor(ox + cx * z), iy, 1, ih, C.grid, cx % (step * 5) == 0 and 0.1 or 0.05) end
        for cy = r0 - r0 % step, r1, step do rect(self, ix, math.floor(oy + cy * z), iw, 1, C.grid, cy % (step * 5) == 0 and 0.1 or 0.05) end
        local xs, ys, cs = g.xs, g.ys, g.cs
        local cw = z >= 10 and z - 1 or z
        local left, right, topp, bottom = ix - z, ix + iw, iy - z, iy + ih
        local percol = self.mapColours
        -- Knox County first, a grey row at a time
        if V then
            for _, r in ipairs(V.runs) do
                local px, py = math.floor(ox + r[2] * z), math.floor(oy + r[1] * z)
                if py > topp and py < bottom then self:drawRect(px, py, (r[3] - r[2] + 1) * z, z, 0.22, C.muted.r, C.muted.g, C.muted.b) end
            end
        end
        for i = 1, g.n do
            local px, py = ox + xs[i] * z, oy + ys[i] * z
            if px > left and px < right and py > topp and py < bottom then
                local c = cs[i] == 0 and C.danger or (V and V.set[(xs[i] + 500) * 10000 + (ys[i] + 500)] and C.warn) or (percol and K.MAP_COLORS[cs[i]] or K.MAP_CELL)
                self:drawRect(math.floor(px), math.floor(py), cw, cw, cs[i] == 0 and 1 or 0.9, c.r, c.g, c.b)
            end
        end
        -- axis labels, on a faint strip along the top and the left edge
        rect(self, ix, iy, iw, H_SMALL + 4, C.panel, 0.85)
        rect(self, ix, iy, width("000") + 10, ih, C.panel, 0.85)
        for cx = c0 - c0 % step, c1, step do
            local px = math.floor(ox + cx * z)
            if px > ix + width("000") + 10 and px < ix + iw - 20 then text(self, tostring(cx), px + 2, iy + 2, C.muted) end
        end
        for cy = r0 - r0 % step, r1, step do
            local py = math.floor(oy + cy * z)
            if py > iy + H_SMALL + 4 and py < iy + ih - H_SMALL then text(self, tostring(cy), ix + 4, py + 1, C.muted) end
        end
        -- the picked cells, then the cell under the mouse
        if self.gridSel then
            for _, k in ipairs(self.gridSel.keys) do
                ring(math.floor(ox + (math.floor(k / 10000) - 500) * z), math.floor(oy + (k % 10000 - 500) * z), C.accentText)
            end
        end
        if rmx >= ix and rmx < ix + iw and my >= cy0 and my < cy1 and not gd then
            local hx, hy = math.floor((rmx - ox) / z), math.floor((rmy - oy) / z)
            local px, py = math.floor(ox + hx * z), math.floor(oy + hy * z)
            ring(px, py, C.onSelected)
            local o = s.owners[(hx + 500) * 10000 + (hy + 500)]
            local t = "Cell " .. hx .. ", " .. hy
            if o then
                for _, e in ipairs(o) do t = t .. "\n" .. self:mapInfo(e.folder).title .. (self:mapShown(e) and "" or " (hidden)") end
                if #o > 1 then t = t .. "\nOverlap: the map higher in the load order wins." end
            elseif not (V and V.set[(hx + 500) * 10000 + (hy + 500)]) then
                t = t .. "\nNo map here"
            end
            if V and V.set[(hx + 500) * 10000 + (hy + 500)] then t = t .. "\nKnox County" .. (o and " (the map over it replaces it here)" or " (the game's own map)") end
            tip(self, t)
        end
    end
    self:clearStencilRect()
    self:setStencilRect(fx, fy, fw, fh)
    if g.n == 0 and not V then
        local msg = s.finished and "Nothing to draw: enable a map mod, or click a map in the legend or the list below." or "Looking for map cells..."
        text(self, msg, ix + math.floor((iw - width(msg)) / 2), iy + math.floor(ih / 2), C.muted)
    end
    if not gd and cy1 > cy0 then self.feedHits[#self.feedHits + 1] = { kind = "griddrag", x = ix, y = cy0, w = iw, h = cy1 - cy0 } end
    -- the legend: every enabled map (and any you turned on), a click shows or hides it
    local lx, ly, lh = x + w - 12 - lgw, y + 12, gh - 24
    local lkey = s.ver .. "|" .. (self.modVer or 0) .. "|" .. (self.showVer or 0)
    if self.legendKey ~= lkey then
        local list = {}
        for _, e in ipairs(s.entries) do
            if e.d.isActive or self.mapShow[e.key] == true then list[#list + 1] = e end
        end
        local at = {}
        for i, f in ipairs(self:maps()) do at[f] = i end
        table.sort(list, function(a, b)
            local pa, pb = at[a.folder] or 1e9, at[b.folder] or 1e9
            if pa ~= pb then return pa < pb end
            return a.key < b.key
        end)
        self.legendKey, self.legendList = lkey, list
    end
    local rh = H_SMALL + 8
    local list = self.legendList
    text(self, "Maps", lx, ly, C.muted)
    ly = ly + H_SMALL + 6
    lh = lh - H_SMALL - 6
    local full = #list * rh
    self.legendScroll = math.max(0, math.min(self.legendScroll or 0, full - lh))
    local ly0, ly1 = math.max(ly, fy), math.min(ly + lh, fy + fh)
    self.legendBox = { x = lx, y = ly0, w = lgw, h = math.max(0, ly1 - ly0) }
    if ly1 > ly0 then
        self:clearStencilRect()
        self:setStencilRect(lx - 4, ly0, lgw + 4, ly1 - ly0)
        for i, e in ipairs(list) do
            local ry = ly + (i - 1) * rh - self.legendScroll
            if ry + rh >= ly and ry <= ly + lh then
                local hy0, hy1 = math.max(ry, ly0), math.min(ry + rh, ly1)
                local over = mx >= lx and mx < lx + lgw - 14 and my >= hy0 and my < hy1
                if over then
                    round(self, lx - 2, ry, lgw - 8, rh, 3, C.hover)
                end
                local sh = self:mapShown(e)
                local c = mapColor(self, e)
                if sh then rect(self, lx + 2, ry + (rh - 12) / 2, 12, 12, c, 1) else outline(self, lx + 2, ry + (rh - 12) / 2, 12, 12, 3, c, 0.8) end
                local info = self:mapInfo(e.folder)
                local cnt = e.done and (#e.cells > 0 and (#e.cells .. "") or "none") or "..."
                text(self, fit(info.title, lgw - 14 - 30 - width(cnt) - 8), lx + 22, ry + 4, sh and C.text or C.muted)
                text(self, cnt, lx + lgw - 14 - 6 - width(cnt), ry + 4, C.muted)
                if over then tip(self, info.title .. "\n" .. e.d.name .. "\n" .. (e.done and (#e.cells > 0 and (#e.cells .. " cells") or "No cells: a spawn-point map, or one that only builds on another.") or "Counting cells...") .. "\nClick to " .. (sh and "hide" or "show") .. " it on the grid.") end
                if hy1 > hy0 then
                    self.feedHits[#self.feedHits + 1] = { kind = "fn", x = lx, y = hy0, w = lgw - 14, h = hy1 - hy0, fn = function()
                        self:mapRemember()
                        self.mapShow[e.key] = not sh
                        self.showVer = (self.showVer or 0) + 1
                    end }
                end
            end
        end
        self:clearStencilRect()
        self:setStencilRect(fx, fy, fw, fh)
    end
    if #list == 0 then text(self, "No map on.", lx, ly, C.muted) end
    scrollbar(self, "legendScroll", lx - 4, ly, lgw + 4, lh, full)
    y = y + gh + 8
    text(self, "Build " .. major .. " cells (" .. (major >= 42 and 256 or 300) .. " tiles). " .. (major >= 42 and "Grey is Knox County, the game's own map; yellow, a mod's cells over it." or "The game's own map isn't drawn on this build."), x, y, C.muted)
    return y + H_SMALL + 16
end

--- The list of every installed map mod, with its search, filter and sort.
function PZMMMods:drawMapMods(x, y, w, mx, my, top, bh)
    local RH = math.max(34, BAR + 4) -- the app's 34 px rows
    local s = self:mapScan()
    local rows = self:mapRows()
    bold(self, T("mapmods_title", "Map mods"), x, y, C.text, MEDIUM)
    local sub = #rows .. " of " .. #s.mods .. (s.found and "" or "   looking...")
    text(self, sub, x + width(T("mapmods_title", "Map mods"), MEDIUM) + 16, y + (H_MEDIUM - H_SMALL) / 2, C.muted)
    y = y + H_MEDIUM + 10
    local bx = x
    local function tool(icon, label, tipText, fn, on)
        local px, py = bx, y
        local tw, over = toolButton(self, icon, label, px, py, mx, my, on)
        if over then tip(self, tipText) end
        self.feedHits[#self.feedHits + 1] = { kind = "fn", fn = function() fn(px, py + BAR + 4) end, x = px, y = py, w = tw, h = BAR }
        bx = bx + tw + 8
    end
    bx = bx + K.searchSpot(self, bx, y, 200, top, bh) + 8
    local fnames = { all = "All maps", on = T("filter_enabled", "Enabled"), installed = T("filter_installed", "Installed"), clash = T("maps_filter_conflict", "Conflicting") }
    local fcur = self.mapFilter or "all"
    tool(TB.filter, fnames[fcur], "Filter\nAll maps, the enabled ones, the installed ones (all of these are), or only those that clash: two maps with the same cells, or the game's own map conflicts. Hide spawn-point mods leaves out mods with no cells.", function(mxp, myp)
        local items = {}
        for _, id in ipairs({ "all", "on", "installed", "clash" }) do
            items[#items + 1] = { fnames[id], function() self.mapFilter = id end, (self.mapFilter or "all") == id }
        end
        items[#items + 1] = false
        items[#items + 1] = { T("maps_hide_spawn", "Hide spawn-point mods"), function() self.mapHideSpawn = not (self.mapHideSpawn ~= false) end, function() return self.mapHideSpawn ~= false end, keep = true }
        self:menu(items, mxp, myp)
    end, fcur ~= "all")
    local sorts = { { "order", T("maps_sort_order", "Load order") }, { "name", T("srv_by_name", "Name") }, { "cells", T("maps_sort_cells", "Most cells") },
        { "clash", T("maps_sort_clash", "Most conflicts") }, { "enabled", T("sort_enabled", "Enabled first") }, { "updated", T("maps_sort_updated", "Recently updated") }, { "newest", T("maps_sort_newest", "Newest") } }
    local sname = T("maps_sort_order", "Load order")
    for _, o in ipairs(sorts) do if o[1] == (self.mapSort or "order") then sname = o[2] end end
    tool(TB.sort, sname, "Sort the list\nEnabled first, load order, name, most cells, most conflicts, or by Steam's update and creation times.", function(mxp, myp)
        local items = {}
        for _, o in ipairs(sorts) do items[#items + 1] = { o[2], function() self.mapSort = o[1] end, (self.mapSort or "order") == o[1] } end
        self:menu(items, mxp, myp)
    end)
    -- the cells picked on the grid filter the list: one chip, a click clears them
    if self.gridSel then
        local names = {}
        for i, k in ipairs(self.gridSel.keys) do
            if i > 3 then break end
            names[#names + 1] = (math.floor(k / 10000) - 500) .. "," .. (k % 10000 - 500)
        end
        local more = #self.gridSel.keys - #names
        local cx, cy = bx + 4, y + math.floor((BAR - H_SMALL - 2) / 2)
        local cr = chip(self, "Cells " .. table.concat(names, "; ") .. (more > 0 and (" +" .. more) or ""), cx, cy, C.accentText)
        if mx >= cx and mx < cr and my >= cy and my < cy + H_SMALL + 2 then tip(self, "Cells on the grid\nThe list shows the maps at these cells. Click to clear. Shift-click the grid adds cells.") end
        self.feedHits[#self.feedHits + 1] = { kind = "fn", fn = function()
            self.gridSel = nil
            self.cellVer = (self.cellVer or 0) + 1
        end, x = cx, y = cy, w = cr - cx, h = H_SMALL + 2 }
    end
    y = y + BAR + 12
    if #rows == 0 then
        local none = s.found and #s.mods == 0
        text(self, none and T("maps_none_build", "No installed mod adds maps for Build %1.", K.myBuild():match("^(%d+)") or "42")
            or s.found and "No map mods match." or "Looking for map mods...", x, y, C.muted)
        return y + H_SMALL
    end
    card(self, x, y, w, #rows * RH)
    local last = #rows
    for i, m in ipairs(rows) do
        local ry = y + (i - 1) * RH
        if ry + RH >= top and ry <= top + bh then
            local d = m.d
            -- a map shown on the grid has the soft highlight
            local onGrid = false
            for _, e in ipairs(m.entries) do if self:mapShown(e) then onGrid = true end end
            if onGrid and self.mapSel ~= m then round(self, x + 1, ry + 1, w - 2, RH - 2, 3, C.accent, 0.16) end
            local over = listRow(self, x, ry, w, RH, i == last, mx, my, self.mapSel == m)
            local mid = ry + (RH - H_SMALL) / 2
            local cbx, cby = x + 16, ry + (RH - BOX) / 2
            checkbox(self, cbx, cby, d.isActive, not d.isAvailable)
            if mx >= cbx - 6 and mx < cbx + BOX + 6 and my >= cby - 6 and my < cby + BOX + 6 then
                tip(self, not d.isAvailable and "Can't load\nThis mod doesn't load on this build of the game." or d.isActive and "Disable" or "Enable")
            end
            self.feedHits[#self.feedHits + 1] = { kind = "fn", fn = function() self:mapRemember(); self:toggle(d) end, x = cbx - 6, y = cby - 6, w = BOX + 12, h = BOX + 12 }
            -- the swatch: its colour on the grid; filled when its cells are drawn
            local sx, sy = x + 44, ry + (RH - 12) / 2
            local shown = onGrid
            local c = mapColor(self, m.entries[1])
            if shown then rect(self, sx, sy, 12, 12, c, 1) else outline(self, sx, sy, 12, 12, 3, c, 0.8) end
            if mx >= sx - 4 and mx < sx + 16 and my >= ry and my < ry + RH then tip(self, "On the grid\nClick to " .. (shown and "hide" or "show") .. " this map's cells.") end
            self.feedHits[#self.feedHits + 1] = { kind = "fn", fn = function() self:mapToggleShow(m) end, x = sx - 4, y = ry, w = 20, h = RH }
            -- right: Swap and Preview, then how many cells
            local right = x + w - 16
            local conf = self:mapConflictsOf(m)
            local nconf = 0
            for _ in pairs(conf) do nconf = nconf + 1 end
            local function rowButton(label, tipText, fn)
                local bw = width(label) + 28
                right = right - bw
                local by2 = ry + (RH - BAR) / 2
                local _, _, o = flatButton(self, label, right, by2, mx, my, "quiet")
                if o then tip(self, tipText) end
                self.feedHits[#self.feedHits + 1] = { kind = "fn", fn = fn, x = right, y = by2, w = bw, h = BAR }
                right = right - 6
            end
            if nconf > 0 then
                rowButton(T("maps_swap", "Swap"), "Swap\nShow this map instead: hide the previewed maps it conflicts with, turn off the enabled ones.", function() self:mapSwap(m) end)
            end
            if not d.isActive then
                rowButton(shown and T("maps_unpreview", "Unpreview") or T("maps_preview", "Preview"), shown and "Unpreview\nTake it off the grid." or "Preview\nShow it on the grid without enabling it: it's checked for conflicts, nothing changes in your mod list.", function() self:mapToggleShow(m) end)
            end
            local total, done = 0, true
            for _, e in ipairs(m.entries) do
                total = total + #e.cells
                if not e.done then done = false end
            end
            local cs = done and (total .. (total == 1 and " cell" or " cells")) or "..."
            if done and total == 0 then cs = "" end
            if cs ~= "" then
                right = right - width(cs)
                text(self, cs, right, mid, over and C.onSelected or C.muted)
                right = right - 16
            end
            local tx = x + 68
            local name = fit(d.name, right - tx - 120)
            bold(self, name, tx, mid, over and C.onSelected or (self.hidden[d.modId] and C.muted or C.text))
            tx = tx + width(name) + 10
            local folders = {}
            for _, e in ipairs(m.entries) do folders[#folders + 1] = e.folder end
            local fl = table.concat(folders, ", ")
            if tx + 80 < right then
                fl = fit(fl, math.min(right - tx - 20, 360))
                text(self, fl, tx, mid, over and C.onSelected or copy(C.muted, 0.75))
                tx = tx + width(fl) + 12
            end
            local cy = ry + (RH - H_SMALL - 2) / 2
            local clash = self:mapClashList(d)
            if #clash > 0 and tx + 110 < right then
                local cx = tx
                tx = chip(self, "Clashes with " .. #clash, tx, cy, C.danger) + 6
                if mx >= cx and mx < tx and my >= cy and my < cy + H_SMALL + 2 then tip(self, "Clashes with\n" .. table.concat(clash, ", ") .. "\nThe one higher in the map order wins.") end
            end
            local dupes = self:mapDupes(d)
            if #dupes > 0 and tx + 110 < right then
                local cx = tx
                tx = chip(self, "Folder also added", tx, cy, C.warn) + 6
                if mx >= cx and mx < tx and my >= cy and my < cy + H_SMALL + 2 then tip(self, "Folder also added\n" .. table.concat(dupes, "\n")) end
            end
            if done and total == 0 and tx + 110 < right then
                local cx = tx
                tx = chip(self, "No cells", tx, cy) + 6
                if mx >= cx and mx < tx and my >= cy and my < cy + H_SMALL + 2 then tip(self, "No cells\nIts folder has no map cells of its own: a spawn-point map, or one that builds on another map.") end
            end
            if over and not (mx >= cbx - 6 and mx < cbx + BOX + 6) then
                local nm = self:mapInfo(m.entries[1].folder)
                tip(self, nm.title .. "\n" .. d.name .. "\n" .. (nm.desc and nm.desc ~= "" and (nm.desc:gsub("<[^>]*>", " "):sub(1, 220)) or "Click for its details."))
            end
            self.feedHits[#self.feedHits + 1] = { kind = "fn", fn = function() self.mapSel = (self.mapSel == m) and nil or m end, x = x, y = ry, w = w, h = RH }
        end
    end
    return y + #rows * RH
end

-- the app's Maps help (its Help button), in the same words
K.MAP_HELP = {
    { "Maps help", "The grid shows enabled maps and the ones you preview. Scroll to zoom, drag to move, click a cell to list the maps there." },
    { "Reading the grid", "Each square is one cell of the world (B42 cells are 256 tiles, B41 cells 300, so only one build's maps compare). Blue is a cell one map fills, red two or more maps fighting over a cell: only one of them loads there. Per-map colours give each shown map its own hue instead." },
    { "Preview or enable", "Enabled maps are always shown. The tick enables a map; to preview it instead, press Preview (or click its swatch). Click its row for its details. A preview is drawn and checked for conflicts, but nothing changes in your mod list until you enable it. Click cells to list the maps there; Shift+click adds more cells." },
    { "Workshop maps", "With the PZMM app, Workshop maps lists every Map-tagged Workshop item for this build, including ones you haven't installed, read from Steam's file lists (no download, nothing installed). Paste a collection in Check a Workshop map to preview all its maps. Preview them like installed maps, then Install the ones you want." },
}
function PZMMMods:drawMaps(x, y, w, mx, my, top, bh)
    y = self:drawNotice(x, y, w, T("notice_maps", "the world picture and hiding the game's map, installing a map's missing requirements, a mod ID in two items, a background picture, the Workshop maps source in the list (Map-tagged items you haven't installed, with its scan and Stop, an Install on those rows and in Compare), searching maps by tag, checking Workshop maps you haven't installed and subscriber counts (Steam's details don't give them)."), mx, my)
    local maps = self:maps()
    local s = self:mapScan()
    text(self, fit("Your enabled maps, in load order: where two maps cover the same cells, the higher one wins. Accept saves it.", w), x, y, C.muted)
    y = y + H_SMALL + 10
    local bx = x
    local function tool(icon, label, tipText, fn, off)
        local px, py = bx, y
        local tw, over = toolButton(self, icon, label, px, py, mx, my, nil, off)
        if over then tip(self, tipText) end
        if not off then self.feedHits[#self.feedHits + 1] = { kind = "fn", fn = function() fn(px, py + BAR + 4) end, x = px, y = py, w = tw, h = BAR } end
        bx = bx + tw + 8
    end
    if #maps > 1 then
        tool(TB.layers, "Autosort maps", "Autosort maps\nA map that builds on another (lots= in its map.info) goes before it; the rest keep their order; Muldraugh, KY last.", function() self:autosortMaps() end)
    end
    tool(TB.copy, T("maps_copy", "Copy"), "Copy\nYour enabled maps for a friend or a server: their Workshop links, or the Mods=, WorkshopItems= and Map= lines.", function(mxp, myp)
        self:menu({
            { "Maps with Workshop links", function() self:mapCopy("links") end },
            { "Mods= line", function() self:mapCopy("mods") end },
            { "WorkshopItems= line", function() self:mapCopy("items") end },
            { "Map= line", function() self:mapCopy("map") end },
            false,
            { "Everything", function() self:mapCopy("all") end },
        }, mxp, myp)
    end)
    tool(TB.layers, T("maps_compare", "Compare with a server"), "Compare with a server\nA hosted server's Map= against your map order: what's missing, what's extra, and the order.", function(mxp, myp)
        local hosted = self:hostedList()
        if #hosted == 0 then return self:say("No hosted servers in Zomboid/Server yet: host one first.") end
        local items = {}
        for _, st in ipairs(hosted) do
            local name = ""
            pcall(function() name = st:getName() end)
            items[#items + 1] = { name, function() self:compareMaps(st) end }
        end
        self:menu(items, mxp, myp)
    end)
    tool(TB.layers, T("maps_presets", "Presets"), "Presets\nSave the maps shown on the grid under a name, and show them again later.", function(mxp, myp) self:mapPresetMenu(mxp, myp) end)
    tool(TB.layers, T("maps_quick", "Most maps that fit"), "Most maps that fit\nPreview the most listed maps that share no cell with each other or the enabled ones.", function() self:mapQuickFit() end)
    local anyOff = false
    for _, m in ipairs(self:mapRows()) do
        if not m.d.isActive then
            for _, e in ipairs(m.entries) do if not self:mapShown(e) then anyOff = true end end
        end
    end
    tool(TB.filter, anyOff and T("maps_preview_listed", "Preview listed") or T("maps_unpreview_listed", "Unpreview listed"), "Preview listed\nShow every listed map you haven't enabled on the grid, or take them off it again. Nothing changes in your mod list.", function() self:mapPreviewListed(anyOff) end)
    tool(TB.undo, T("maps_undo", "Undo"), "Undo\nUndo the last preview change, quick select, preset, order change or enable/disable on this page.", function() self:mapUndo() end, not (self.mapUndos and #self.mapUndos > 0))
    tool(TB.info, T("maps_help", "Help"), "Help\nHow to read the grid, preview or enable a map, and what Workshop maps do.", function() self.mapHelp = not self.mapHelp end)
    y = y + BAR + 12
    if self.mapHelp then
        local iw = w - 40
        local hc = self.mapHelpC
        if not hc or hc.iw ~= iw or hc.font ~= H_SMALL then
            hc = { iw = iw, font = H_SMALL, rows = {} }
            for _, sec in ipairs(K.MAP_HELP) do
                hc.rows[#hc.rows + 1] = { sec[1], true }
                for _, l in ipairs(wrap(sec[2], iw)) do hc.rows[#hc.rows + 1] = { l } end
            end
            self.mapHelpC = hc
        end
        local ph = 32 + #hc.rows * (H_SMALL + 3) + BAR + 16
        card(self, x, y, w, ph)
        local py = y + 16
        for _, r in ipairs(hc.rows) do
            if r[2] then bold(self, r[1], x + 20, py, C.text) else text(self, r[1], x + 20, py, C.muted) end
            py = py + H_SMALL + 3
        end
        local cw2, _, co = flatButton(self, "Close", x + 20, py + 8, mx, my, "quiet", TB.x)
        self.feedHits[#self.feedHits + 1] = { kind = "fn", fn = function() self.mapHelp = nil end, x = x + 20, y = py + 8, w = cw2, h = BAR }
        y = y + ph + 12
    end
    -- bulk: what's previewed turned on, what's listed and on turned off, previews cleared, hidden maps back
    do
        local rows, previewOff, listedOn, hiddenN = self:mapRows(), {}, {}, 0
        for _, m in ipairs(rows) do
            if m.d.isActive then
                listedOn[#listedOn + 1] = m.d
            else
                for _, e in ipairs(m.entries) do
                    if self:mapShown(e) then previewOff[#previewOff + 1] = m.d; break end
                end
            end
        end
        for _, m in ipairs(s.mods) do if self.hidden[m.d.modId] then hiddenN = hiddenN + 1 end end
        local anyPreview = false
        for _, m in ipairs(s.mods) do
            if not m.d.isActive then
                for _, e in ipairs(m.entries) do if self.mapShow[e.key] then anyPreview = true end end
            end
        end
        bx = x
        local bulk = {}
        if anyPreview then bulk[#bulk + 1] = { T("maps_enabled_only", "Enabled only"), "Enabled only\nTake every preview off the grid: only the enabled maps stay.", function()
            self:mapRemember()
            for _, m in ipairs(s.mods) do
                if not m.d.isActive then for _, e in ipairs(m.entries) do self.mapShow[e.key] = nil end end
            end
            self.showVer = (self.showVer or 0) + 1
        end } end
        if #previewOff > 0 then bulk[#bulk + 1] = { "Enable " .. #previewOff .. " previewed", "Enable previewed\nTurns on the listed maps you are previewing.", function()
            self:mapRemember()
            self:setMany(previewOff, {})
        end, TB.check } end
        if #listedOn > 0 then bulk[#bulk + 1] = { "Disable " .. #listedOn .. " listed", "Disable listed\nTurns off the enabled maps in the list.", function()
            self:mapRemember()
            self:setMany({}, listedOn)
        end, TB.x } end
        if hiddenN > 0 then bulk[#bulk + 1] = { "Unhide " .. hiddenN, "Unhide\nPuts the maps you hid back in the list.", function()
            for _, m in ipairs(s.mods) do self.hidden[m.d.modId] = nil end
            writeHidden(self.hidden)
            self:updateView()
        end } end
        if #bulk > 0 then
            for _, b in ipairs(bulk) do
                local bw, bh2, over = flatButton(self, b[1], bx, y, mx, my, nil, b[4])
                if over then tip(self, b[2]) end
                self.feedHits[#self.feedHits + 1] = { kind = "fn", fn = b[3], x = bx, y = y, w = bw, h = bh2 }
                bx = bx + bw + 8
            end
            y = y + BAR + 12
        end
    end
    if self.compared then y = self:drawCompare(x, y, w, mx, my) end
    y = self:drawMapOrder(x, y, w, mx, my) + 20
    y = self:drawGrid(x, y, w, mx, my)
    y = self:drawMapReqsAll(x, y, w, mx, my)
    if self.mapSel and s.modOf[self.mapSel.d.modId] then
        y = self:drawMapStrip(x, y, w, mx, my)
        y = self:drawMapReqs(x, y, w, mx, my)
    else
        self.mapSel = nil
    end
    return self:drawMapMods(x, y, w, mx, my, top, bh)
end

function PZMMMods:drawMapOrder(x, y, w, mx, my)
    local RH = math.max(34, BAR + 4) -- the app's 34 px rows
    local maps = self:maps()
    if #maps == 0 then
        text(self, "No maps: enable a map mod in My mods.", x, y, C.muted)
        return y + H_SMALL
    end
    local h = #maps * RH
    card(self, x, y, w, h)
    -- a drag by the number: past 5 px it's a move, landing before the row nearest the mouse (never after
    -- the game's own map, which stays last)
    local md = self.mapDrag
    local lastFixed = maps[#maps] == K.BASE_MAP
    if md and not md.moved and math.abs(my - md.y0) >= 5 then md.moved = true end
    md = md and md.moved and md or nil
    if md then
        md.to = nil
        if my > -1e8 then md.to = math.max(1, math.min(lastFixed and #maps or #maps + 1, math.floor((my - y) / RH + 0.5) + 1)) end
    end
    for i, m in ipairs(maps) do
        local ry = y + (i - 1) * RH
        listRow(self, x, ry, w, RH, i == #maps, mx, my, nil, true)
        local mid = ry + (RH - H_SMALL) / 2
        local over = not md and mx >= x and mx < x + w and my >= ry and my < ry + RH
        text(self, tostring(i), x + 36 - width(tostring(i)), mid, over and C.onSelected or C.accentText)
        if m ~= K.BASE_MAP and #maps > 1 then
            if mx >= x and mx < x + 48 and my >= ry and my < ry + RH and not md then tip(self, "Drag to sort\nMove it up or down the load order.") end
            self.feedHits[#self.feedHits + 1] = { kind = "mapdrag", i = i, x = x, y = ry, w = 48, h = RH }
        end
        -- its title, then its folder (what Map= lists)
        local title = self:mapInfo(m).title
        bold(self, title, x + 52, mid, C.text)
        if title ~= m then text(self, m, x + 52 + width(title) + 12, mid, copy(C.muted, 0.75)) end
        local nameW = width(title) + (title ~= m and width(m) + 12 or 0)
        local clash = {}
        pcall(function()
            local c = self.model:getMapConflicts(m)
            for j = 0, c:size() - 1 do clash[#clash + 1] = c:get(j) end
        end)
        if #clash > 0 then
            local cx = x + 52 + nameW + 14
            local cy = ry + (RH - H_SMALL - 2) / 2
            local right = chip(self, "Clashes with " .. #clash, cx, cy, C.danger)
            if mx >= cx and mx < right and my >= cy and my < cy + H_SMALL + 2 then tip(self, "Clashes with\n" .. table.concat(clash, ", ") .. "\nThe one higher in the list wins.") end
        end
        -- Top, Up and Down at the right, as the app's; the game's own map is fixed last
        local bx = x + w - 16
        if m == K.BASE_MAP then
            local right = pill(self, "Last", bx, ry + (RH - H_SMALL - 4) / 2, C.muted)
            if mx >= right and mx < bx and my >= ry and my < ry + RH then tip(self, "Last\nThe game's own map always loads last.") end
        else
            local below = maps[i + 1] and maps[i + 1] ~= K.BASE_MAP
            -- Top as text, Up and Down as chevron icon buttons
            for _, b in ipairs({ { "Down", i + 1, below, "Move down\nLoads after the map under it.", TB.chev_down }, { "Up", i - 1, i > 1, "Move up\nLoads before the map above it.", TB.chev_up }, { "Top", 1, i > 1, "Top\nLoad it first." } }) do
                if b[3] then
                    local bw = b[5] and BAR or (width(b[1]) + 28)
                    bx = bx - bw
                    local by = ry + (RH - BAR) / 2
                    local over
                    if b[5] then
                        local _, o = toolButton(self, b[5], nil, bx, by, mx, my)
                        over = o
                    else
                        local _, _, o = flatButton(self, b[1], bx, by, mx, my, "quiet")
                        over = o
                    end
                    if over then tip(self, b[4]) end
                    local from, to = i, b[2]
                    self.feedHits[#self.feedHits + 1] = { kind = "fn", fn = function() self:moveMap(from, to) end, x = bx, y = by, w = bw, h = BAR }
                    bx = bx - 4
                end
            end
        end
        if md and md.i == i then rect(self, x, ry, w, RH, C.panel, 0.6) end
    end
    if md and my > -1e8 then
        if md.to and md.to ~= md.i and md.to ~= md.i + 1 then
            local ty = y + (md.to - 1) * RH
            if K.FEATHER then self:drawTextureScaled(K.FEATHER, x + 1, ty, w - 2, 20, 1, C.accent.r, C.accent.g, C.accent.b) end
            rect(self, x + 1, ty, w - 2, 1, C.dropEdge, 1)
        end
        local gw, gy = math.min(w, 420), my - math.floor(RH / 2)
        round(self, mx - 16, gy, gw, RH, 6, C.panel, 0.85)
        outline(self, mx - 16, gy, gw, RH, 6, C.accent, 0.7)
        bold(self, fit(self:mapInfo(maps[md.i]).title, gw - 24), mx - 4, gy + (RH - H_SMALL) / 2, C.text)
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
                local s = { folder = k, info = info, active = info.activeMods, worldVersion = info.worldVersion, alive = info.playerAlive, mapName = info.mapName, name = info.saveName or k, mode = info.gameMode or "", map = info.mapName or "Muldraugh, KY", played = getLastPlayedDate(k) or "", mods = {}, missing = {} }
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

--- How a save's mods differ from the ones on now ("2 more, 1 fewer"), or nil when they're the same.
function PZMMMods:modsDiffer(s)
    local on, theirs, more, fewer = {}, {}, 0, 0
    for _, id in ipairs(self.order or {}) do on[id] = true end
    for _, id in ipairs(s.mods) do
        theirs[id] = true
        if not on[id] then more = more + 1 end
    end
    for id in pairs(on) do if not theirs[id] then fewer = fewer + 1 end end
    if more == 0 and fewer == 0 then return nil end
    local parts = {}
    if more > 0 then parts[#parts + 1] = more .. (more == 1 and " mod it uses is" or " mods it uses are") .. " off in My mods" end
    if fewer > 0 then parts[#parts + 1] = fewer .. (fewer == 1 and " mod on in My mods isn't" or " mods on in My mods aren't") .. " in it" end
    return table.concat(parts, "; ") .. "."
end

-- the app's Reset choices, as the game's own save file operations
K.RESETS = {
    { "zombies", "Zombie population", "The zombies are spread again from the map and your sandbox settings, as in a new world. Areas you cleared can fill up again.", { "DeleteZPopVirtualBin", "DeleteZPopXYBin" } },
    { "time", "Clock, calendar and weather", "map_t.bin is removed: the date and time go back to the save's start and the weather starts fresh. What counts from the start of the world, like the power and water shutoff and loot respawn, counts from there again.", { "DeleteMapTBin" } },
    { "animals", "Animals", "The animals are built again from the map. Any animal, tamed ones too, may be lost.", { "DeleteAPopXYBin" } },
}

--- A save's More menu: Characters, Rename, Recovery, Give it my mods, Folder, Back up in PZMM, Delete.
function PZMMMods:saveMenu(s)
    local items = {
        { "Characters...", function() self:characterMenu(s) end },
        { "Rename...", function() self:renameSave(s) end },
        { "Recovery...", function()
            local sub = {}
            for _, r in ipairs(K.RESETS) do sub[#sub + 1] = { "Reset " .. r[2]:lower(), function() self:resetSave(s, r) end } end
            self:menu(sub)
        end },
        false,
        { "Give it my mods", function() self:giveMods(s) end },
        { "Folder", function() pcall(function() showFolderInDesktop(getAbsoluteSaveFolderName(s.folder)) end) end },
    }
    if self.app then
        items[#items + 1] = { "Back up in PZMM", function() self:request("backup\t" .. s.folder:gsub("\\", "/"), "Backup of " .. s.name .. " asked for.") end }
        local backups = readBackups()[s.folder:gsub("\\", "/")] or {}
        if #backups > 0 then
            items[#items + 1] = { "Restore a backup...", function()
                local sub = {}
                for i, b in ipairs(backups) do
                    if i > 12 then break end
                    local days = math.floor((now() - b.at) / 86400)
                    local when = days <= 0 and "today" or days == 1 and "yesterday" or (days .. " days ago")
                    sub[#sub + 1] = { (b.slot ~= "" and (b.slot .. ", ") or "") .. when, function()
                        self:request("restore\t" .. s.folder:gsub("\\", "/") .. "\t" .. b.file, "Restoring " .. s.name .. ": say yes in PZMM.")
                    end }
                end
                self:menu(sub)
            end }
        end
    end
    items[#items + 1] = false
    items[#items + 1] = { "Delete", function() self:deleteSave(s) end, danger = true }
    self:menu(items)
end

--- Who the save loads as: one of its living characters, or a new one (the game's own Load screen's choice).
function PZMMMods:characterMenu(s)
    local items = {}
    for _, pl in ipairs(s.info and s.info.players or {}) do
        if not pl.isDead then
            items[#items + 1] = { "Play as " .. tostring(pl.name), function()
                pcall(function() setSavefilePlayer1(s.mode, s.name, pl.sqlID) end)
                self.saves = nil
                self:say(s.name .. " loads as " .. tostring(pl.name) .. " now.")
            end, pl.sqlID == 1 }
        end
    end
    if s.mode ~= "A Really CD DA" then
        if #items > 0 then items[#items + 1] = false end
        items[#items + 1] = { "New character", function()
            pcall(function() setSavefilePlayer1(s.mode, s.name, -1) end)
            self.saves = nil
            self:say(s.name .. ": Play makes a new character.")
        end }
    end
    if #items == 0 then return self:say(s.name .. " has no living characters.") end
    self:menu(items)
end

--- Rename a save, as the game's Load screen: a name the game accepts, not taken.
function PZMMMods:renameSave(s)
    self:prompt("Rename " .. s.name, s.name, "Rename", function(name)
        name = (name or ""):gsub("^%s+", ""):gsub("%s+$", "")
        if name == "" or name == s.name then return end
        local ok = false
        pcall(function()
            ok = sanitizeWorldName(name) == name and name:sub(1, 1) ~= "." and name:sub(-1) ~= "."
                and not checkSaveFolderExists(s.mode .. getFileSeparator() .. name)
        end)
        if not ok then return self:say("A save can't be called that: use letters, numbers and spaces, and a name no other save has.") end
        local done = false
        pcall(function() done = renameSavefile(s.mode, s.name, name) end)
        self.saves = nil
        self:say(done and (s.name .. " is now " .. name .. ".") or ("Couldn't rename " .. s.name .. ": is it open somewhere?"))
    end)
end

--- Reset part of a save so the game builds it again. With PZMM it backs the save up first (and the reset
--- waits for that); without it, it asks knowing there's no going back.
function PZMMMods:resetSave(s, r)
    local function reset()
        for _, op in ipairs(r[4]) do pcall(function() manipulateSavefile(s.folder, op) end) end
        self.saves = nil
        self:say(r[2] .. " reset in " .. s.name .. ". Load the save and the game builds it again.")
    end
    local backup = self.app ~= nil
    self:ask({ title = "Reset the " .. r[2]:lower() .. " of " .. s.name .. "?", lines = { r[3], backup and "PZMM backs the save up first, so you can go back." or "There's no backup without PZMM: this can't be undone." }, choices = {
        { "Reset", function()
            if not backup then return reset() end
            local id = self:request("backup\t" .. s.folder:gsub("\\", "/"), "Backing up " .. s.name .. " first: say yes in PZMM.")
            if id then self.afterAck[id] = { reset, "The reset waits for the backup", nil, { "Reset without a backup", reset } } end
        end },
    } })
end

--- Why a save can't be played from here (the game's own Load screen's reasons), or nil.
function PZMMMods:cantPlay(s)
    if s.mode == "Multiplayer" then return "Can't play here\nMultiplayer saves load from the server browser." end
    if s.mode == "LastStand" then return "Can't play here\nLast Stand has no saves to load." end
    local v = tonumber(s.worldVersion)
    if not v or not s.mapName then return "Can't play\nThis save is missing its world details, so the game can't load it." end
    local now = 0
    pcall(function() now = IsoWorld.getWorldVersion() end)
    if now > 0 and v > now then return "Can't play\nThis save is from a newer version of the game." end
    if v <= 200 then return "Can't play\nThis save is from an old version of the game it can't load any more." end
    if s.mode == "A Really CD DA" and not s.alive then return "Can't play\nThis character died, so the save can't go on." end
end

--- My mods become this save's: each mod on exactly when the save uses it (Accept keeps it, Back undoes it).
function PZMMMods:useSaveMods(s)
    -- (the mod list and map order as they were are kept from open, for Back's "Don't save")
    if s.active then
        -- the game's own copy: its mods in their order, and its map order
        pcall(function() self.model:getActiveMods():copyFrom(s.active) end)
        self.model:refreshMods()
        self.diffAt = nil
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
    local function delete()
        deleteSave(s.folder)
        self.saves, self.latest = nil, nil
        -- the main menu's Continue and Load follow, as after the game's own Delete
        pcall(function()
            local m = MainScreen.instance
            m:getLatestSave()
            if #getFullSaveDirectoryTable() == 0 then
                m.loadOption:setVisible(false)
                m.latestSaveOption:setVisible(false)
            end
        end)
        self:say(s.name .. " deleted.")
    end
    -- with PZMM: back it up first (the delete waits for the backup), or delete it outright
    local choices = {}
    if self.app then
        choices[#choices + 1] = { "Back up first, then delete", function()
            local id = self:request("backup\t" .. s.folder:gsub("\\", "/"), "Backing up " .. s.name .. " first: say yes in PZMM.")
            if id then self.afterAck[id] = { delete, "Not deleted, the backup didn't happen" } end
        end }
    end
    choices[#choices + 1] = { "Delete", delete }
    self:ask({ title = "Delete " .. s.name .. "?" .. (self.app and "" or " This can't be undone."), lines = self.app and { "PZMM keeps a backup if you ask it to." } or nil, choices = choices })
end

function PZMMMods:drawSaves(x, y, w, mx, my)
    y = self:drawNotice(x, y, w, T("notice_saves", "backup screenshots, managing backups (delete, rename, named slots), automatic backups, Copy as and Copy with my mods (the game can't copy folders), repairing empty map files, characters' positions and data, the sandbox editor, the Data tab, deleted saves and listing other broken files."), mx, my)
    local saves = self:savesList()
    if #saves == 0 then
        text(self, "No saves yet.", x, y, C.muted)
        return y + H_SMALL
    end
    -- a row holds the name, the meta line and the chips (with 14 px above and below), or the picture
    local th = 64
    local rh = math.max(th + 24, 14 + H_MEDIUM + 4 + H_SMALL + 6 + H_SMALL + 2 + 14)
    -- the card's heading and a lede, above the rows
    local head = 16 + H_MEDIUM + 4 + H_SMALL + 14
    local h = head + #saves * rh
    card(self, x, y, w, h)
    bold(self, "Saves", x + 16, y + 16, C.text, MEDIUM)
    text(self, "Every save, with Play, Use its mods and More.", x + 16, y + 16 + H_MEDIUM + 4, C.muted)
    rect(self, x, y + head - 1, w, 1, C.line, 0.6)
    y = y + head
    for i, s in ipairs(saves) do
        local ry = y + (i - 1) * rh
        listRow(self, x, ry, w, rh, i == #saves, mx, my)
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
        -- Play and Use its mods; the rest in More
        local buttons = {
            { "More", "quiet", "More\nCharacters, Rename, Recovery, Give it my mods, Folder" .. (self.app and ", Back up in PZMM" or "") .. " and Delete.", function() self:saveMenu(s) end },
            { "Use its mods", nil, "Use its mods\nTurns on exactly this save's mods in My mods. Accept keeps them.", function() self:useSaveMods(s) end },
            { "Play", "primary", self:cantPlay(s) or "Play\nLoad this save with its own mods.", function()
                local why = self:cantPlay(s)
                if why then return self:say(why:gsub("\n", ": ")) end
                self:leaveTo(function() MainScreen.continueLatestSave(s.mode, s.name) end)
            end, TB.play },
        }
        -- the text stops before the buttons (8 px apart)
        local bwAll = 0
        for _, b in ipairs(buttons) do bwAll = bwAll + width(b[1]) + 28 + (b[5] and 20 or 0) + 8 end
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
            cx = right
        end
        -- its mods aren't the ones on in My mods now
        if s.active then
            local diff = self:modsDiffer(s)
            if diff then
                local dx = cx + 6
                local right = chip(self, "Mods differ", dx, cy, C.warn)
                if mx >= dx and mx < right and my >= cy and my < cy + H_SMALL + 2 then tip(self, "Mods differ\n" .. diff .. "\nPlay loads the save with its own mods. Use its mods or Give it my mods makes them match.") end
            end
        end
        -- Play, Folder, Delete at the right
        local bx = x + w - 16
        local by = ry + (rh - BAR) / 2
        for _, b in ipairs(buttons) do
            local bw = width(b[1]) + 28 + (b[5] and 20 or 0)
            bx = bx - bw
            local _, _, over = flatButton(self, b[1], bx, by, mx, my, b[2], b[5])
            if over then tip(self, b[3]) end
            self.feedHits[#self.feedHits + 1] = { kind = "fn", fn = b[4], x = bx, y = by, w = bw, h = BAR }
            bx = bx - 8
        end
    end
    return y + #saves * rh
end

-- Servers ---------------------------------------------------------------------------------------------

-- the Internet list, as Steam answers (the game's server browser asks the same way)
PZMMMods.inet = PZMMMods.inet or {}
local function onServer(i)
    pcall(function()
        local sv = steamGetInternetServerDetails(i)
        if sv then PZMMMods.inet[#PZMMMods.inet + 1] = sv end
    end)
end
--- Listen for Steam's answers (the event exists once the game's server browser code has loaded).
local function listen()
    if not PZMMMods.listening and Events.OnSteamServerResponded then
        Events.OnSteamServerResponded.Add(onServer)
        PZMMMods.listening = true
    end
end
PZMMMods.listenInet = listen

--- A Mods= list (; or , between): the ids, and those not installed.
-- The PZMM list in a server's description ([PZMM:1:<len>:<crc>:<data>], the data deflated, URL-safe base64): the app
-- keeps which mods are "server only" and which "optional" there. Arithmetic only (Kahlua has no bit operators).
K.P2 = {}
for i = 0, 24 do K.P2[i] = 2 ^ i end
K.B64 = {}
do
    local a = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_"
    for i = 1, #a do K.B64[a:sub(i, i)] = i - 1 end
end
K.LBASE = { 3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258 }
K.LEXT = { 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0 }
K.DBASE = { 1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577 }
K.DEXT = { 0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13 }
K.CLORD = { 16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15 }

--- A Huffman table from code lengths 0..n-1 (as zlib's puff builds it).
function K.huff(lengths, n)
    local count, offs, symbol = {}, {}, {}
    for l = 0, 15 do count[l] = 0 end
    for i = 0, n - 1 do count[lengths[i]] = count[lengths[i]] + 1 end
    offs[1] = 0
    for l = 1, 14 do offs[l + 1] = offs[l] + count[l] end
    for i = 0, n - 1 do
        local l = lengths[i]
        if l ~= 0 then symbol[offs[l]] = i; offs[l] = offs[l] + 1 end
    end
    return { count = count, symbol = symbol }
end

--- Inflate raw deflate bytes (a table of byte values) to a string; nil when damaged (never raises).
function K.inflate(src)
    local pos, bitbuf, bitcnt, out, n = 1, 0, 0, {}, 0
    local bad = false
    local P2 = K.P2
    local function bits(need)
        if bad then return 0 end
        while bitcnt < need do
            local b = src[pos]
            if not b then bad = true return 0 end
            pos = pos + 1
            bitbuf = bitbuf + b * P2[bitcnt]
            bitcnt = bitcnt + 8
        end
        local v = bitbuf % P2[need]
        bitbuf = math.floor(bitbuf / P2[need])
        bitcnt = bitcnt - need
        return v
    end
    local function decode(h)
        local code, first, index = 0, 0, 0
        for len = 1, 15 do
            code = code + bits(1)
            if bad then return nil end
            local c = h.count[len]
            if code - c < first then
                local sym = h.symbol[index + (code - first)]
                if not sym then bad = true end
                return sym
            end
            index = index + c
            first = (first + c) * 2
            code = code * 2
        end
        bad = true
        return nil
    end
    local function codes(lc, dc)
        while not bad do
            local sym = decode(lc)
            if bad then return end
            if sym < 256 then
                n = n + 1
                out[n] = sym
            elseif sym == 256 then
                return
            else
                sym = sym - 257
                if sym >= 29 then bad = true return end
                local len = K.LBASE[sym + 1] + bits(K.LEXT[sym + 1])
                local ds = decode(dc)
                if bad then return end
                if ds >= 30 then bad = true return end
                local dist = K.DBASE[ds + 1] + bits(K.DEXT[ds + 1])
                if bad or dist > n then bad = true return end
                for _ = 1, len do
                    n = n + 1
                    out[n] = out[n - dist]
                end
            end
        end
    end
    local last = 0
    while last == 0 and not bad do
        last = bits(1)
        local typ = bits(2)
        if bad then break end
        if typ == 0 then
            bitbuf, bitcnt = 0, 0
            if not (src[pos] and src[pos + 1]) then bad = true break end
            local len = src[pos] + src[pos + 1] * 256
            pos = pos + 4
            for _ = 1, len do
                local b = src[pos]
                if not b then bad = true break end
                n = n + 1
                out[n] = b
                pos = pos + 1
            end
        elseif typ == 1 then
            local l = {}
            for i = 0, 143 do l[i] = 8 end
            for i = 144, 255 do l[i] = 9 end
            for i = 256, 279 do l[i] = 7 end
            for i = 280, 287 do l[i] = 8 end
            local d = {}
            for i = 0, 29 do d[i] = 5 end
            codes(K.huff(l, 288), K.huff(d, 30))
        elseif typ == 2 then
            local nlen, ndist, ncode = bits(5) + 257, bits(5) + 1, bits(4) + 4
            local cl = {}
            for i = 0, 18 do cl[i] = 0 end
            for i = 1, ncode do cl[K.CLORD[i]] = bits(3) end
            if bad then break end
            local lc = K.huff(cl, 19)
            local ln, idx = {}, 0
            while idx < nlen + ndist and not bad do
                local sym = decode(lc)
                if bad then break end
                if sym < 16 then
                    ln[idx] = sym
                    idx = idx + 1
                else
                    local len, rep = 0, 0
                    if sym == 16 then
                        len, rep = ln[idx - 1], 3 + bits(2)
                        if not len then bad = true break end
                    elseif sym == 17 then rep = 3 + bits(3)
                    else rep = 11 + bits(7) end
                    for _ = 1, rep do
                        ln[idx] = len
                        idx = idx + 1
                    end
                end
            end
            if bad or idx > nlen + ndist then bad = true break end
            local d = {}
            for i = 0, ndist - 1 do d[i] = ln[nlen + i] end
            codes(K.huff(ln, nlen), K.huff(d, ndist))
        else
            bad = true
        end
    end
    if bad then return nil end
    local parts = {}
    for i = 1, n do parts[i] = string.char(out[i]) end
    return table.concat(parts)
end

--- The mods a server's description tags "server only" and "optional": two sets of mod ids (empty without the tag).
function K.tagSides(desc)
    local serverOnly, optional = {}, {}
    local len, data
    pcall(function()
        local at
        local i = 0
        while true do
            local j = tostring(desc):find("[PZMM:1:", i + 1, true)
            if not j then break end
            at, i = j, j
        end
        if at then len, data = tostring(desc):sub(at):match("^%[PZMM:1:(%d+):%x+:([%w%-_]+)") end
    end)
    len = tonumber(len)
    if not (len and data and #data >= len) then return serverOnly, optional end
    local bytes, acc, nb = {}, 0, 0
    for i = 1, len do
        local v = K.B64[data:sub(i, i)]
        if not v then return serverOnly, optional end
        acc, nb = acc * 64 + v, nb + 6
        if nb >= 8 then
            nb = nb - 8
            bytes[#bytes + 1] = math.floor(acc / K.P2[nb])
            acc = acc % K.P2[nb]
        end
    end
    local text = K.inflate(bytes)
    if not text then return serverOnly, optional end
    local lines = {}
    for l in (text .. "\n"):gmatch("([^\n]*)\n") do lines[#lines + 1] = l end
    for id in (lines[4] or ""):gmatch("[^;]+") do serverOnly[id] = true end
    for id in (lines[5] or ""):gmatch("[^;]+") do optional[id] = true end
    return serverOnly, optional
end

function PZMMMods:modIds(str)
    local ids, missing = {}, {}
    for id in tostring(str or ""):gmatch("[^;,]+") do
        id = id:gsub("^%s*\\?", ""):gsub("%s+$", "")
        if id ~= "" then
            ids[#ids + 1] = id
            if not self.model.mods[id] then missing[#missing + 1] = id end
        end
    end
    return ids, missing
end

--- A server's mods (its Mods= list): installed or not.
function PZMMMods:serverMods(sv)
    local str = ""
    pcall(function() str = sv:getMods() or "" end)
    return self:modIds(str)
end

--- Join a saved server with its saved account (several accounts: the game's server browser picks).
function PZMMMods:joinSaved(sv)
    local name, ip, port, accounts = "", "", 16261, {}
    pcall(function()
        name, ip, port = sv:getName() or "", sv:getIp() or "", sv:getPort() or 16261
        local a = sv:getAccounts()
        for j = 0, a:size() - 1 do accounts[#accounts + 1] = a:get(j) end
    end)
    if #accounts ~= 1 then return self:gameScreen("MULTIPLAYER") end
    local acc = accounts[1]
    local u, pw, spw = "", "", ""
    pcall(function() u, pw, spw = acc:getUserName(), acc:getPwd(), sv:getServerPassword() or "" end)
    self:beforeJoin(function(prev)
        pcall(function() ConnectToServer.instance:connect(prev, name, u, pw, ip, "", tostring(port), spw, false, false, acc:getAuthType()) end)
    end)
end
--- Connect from this screen: with changes to your mods not accepted it asks first, as leaving does (the game's
--- connect screen hides this one and comes back to `prev`: a stand-in that opens it again when this one closed).
function PZMMMods:beforeJoin(go)
    if self:changes(true) == 0 then return go(self) end
    local back = { setVisible = function(_, v) if v then PZMMMods.open() end end }
    self:leaveTo(function() go(back) end)
end

--- The servers you joined lately, as PZMM noted them (Zomboid/Lua/pzmm/servers.txt: name, ip:port, when).
function PZMMMods:recentServers()
    if not self.recent then
        local recent = {}
        self.recent = recent
        pcall(function()
            local r = getFileReader("pzmm/servers.txt", false)
            if not r then return end
            local line = r:readLine()
            while line do
                local name, addr, at = line:match("^([^\t]*)\t([^\t]+)\t(%d+)")
                if addr then recent[#recent + 1] = { name = name, addr = addr, at = tonumber(at) } end
                line = r:readLine()
            end
            r:close()
        end)
    end
    return self.recent
end

-- what Steam says about a server's rules (description, version, mods, last wipe), by "host:port": the game's
-- own server browser asks the same way (steamRequestServerRules) and listens to the same event
PZMMMods.rulesBy = PZMMMods.rulesBy or {}
local function onRules(host, port, rules)
    pcall(function()
        local e = {
            description = rules.description, version = rules.version, mods = rules.mods, items = rules.workshop or rules.workshopItems,
            lastWipe = tonumber(rules.lastWipe), open = rules.open == "1", modCount = tonumber(rules.modCount),
        }
        if rules.public ~= nil then e.public = rules.public == "1" or rules.public == "true" or rules.public == true end
        -- PvP, when Steam's rules say
        local pv = rules.pvp
        if pv ~= nil then e.pvp = pv == "1" or pv == "true" or pv == true end
        PZMMMods.rulesBy[tostring(host) .. ":" .. tostring(port)] = e
    end)
end
--- Listen for Steam's rules (the event exists once the game's server browser code has loaded).
local function listenRules()
    if not PZMMMods.listeningRules and Events.OnSteamRulesRefreshComplete then
        Events.OnSteamRulesRefreshComplete.Add(onRules)
        PZMMMods.listeningRules = true
    end
end

--- Pick a server: its details open at the top of Servers, and Steam is asked for its rules.
function PZMMMods:selectServer(sel)
    sel.key = sel.key or ((sel.ip or "") .. ":" .. tostring(sel.port or ""))
    self.srvSel = sel
    self.feedScroll = 0
    if sel.ip and not sel.hosted then self:askRules(sel) end
    -- no server object (an address typed): players and ping come from Steam's own query
    if sel.ip and not sel.hosted and not sel.sv then
        listenResponded2()
        pcall(function() steamRequestServerDetails(hostOf(sel.ip), tonumber(sel.port) or 16261) end)
    end
end

function PZMMMods:askRules(sel)
    listenRules()
    pcall(function()
        local host = hostOf(sel.ip)
        sel.host = host
        steamRequestServerRules(host, tonumber(sel.port) or 16261)
    end)
end

--- The game's build as "42.13.1", and its first two numbers (servers on another build turn you away).
function K.myBuild()
    if not K.build then
        K.build = ""
        pcall(function() K.build = tostring(getCore():getVersion() or "") end)
        if K.build == "" then pcall(function() K.build = tostring(getGameVersion() or "") end) end
    end
    return K.build
end
function K.buildKey(v) return tostring(v):match("^(%d+%.?%d*)") or tostring(v) end
--- Bytes as "12.3 MB".
function K.mb(b)
    if b >= 1073741824 then return (math.floor(b / 107374182.4) / 10) .. " GB" end
    return b >= 1048576 and ((math.floor(b / 104857.6) / 10) .. " MB") or (math.floor(b / 1024) .. " KB")
end
--- The game's saved servers (favourites) by "ip:port", read twice a second.
function K.favs(ui)
    local t = getTimestampMs()
    local c = ui.favCache
    if c and t - c.at < 500 then return c.by end
    local by = {}
    pcall(function()
        for _, sv in ipairs(getServerList() or {}) do
            by[tostring(sv:getIp()) .. ":" .. tostring(sv:getPort())] = sv
        end
    end)
    ui.favCache = { at = t, by = by }
    return by
end
--- The saved server that is this one (nil: not a favourite).
function K.favKey(ui, sel) return K.favs(ui)[tostring(sel.ip) .. ":" .. tostring(sel.port or 16261)] end
--- Add a server to the game's saved servers, or take it out (as its server browser's star does).
function PZMMMods:toggleFavourite(sel)
    local saved = K.favKey(self, sel)
    if saved then
        pcall(function() deleteServerToAccountList(saved) end)
        self:say((sel.name ~= "" and sel.name or tostring(sel.ip)) .. " is no longer a favourite.")
    else
        local sv = sel.sv
        local pw = false
        if sv then pcall(function() pw = sv:isPasswordProtected() end) end
        if not sv then return self:say("Find it in the Internet list to favourite it, or add it in the game's server browser.") end
        if pw then return self:say("It has a password: add it in the game's server browser, which asks for it.") end
        pcall(function() addServerToAccountList(sv) end)
        self:say((sel.name ~= "" and sel.name or tostring(sel.ip)) .. " is a favourite now.")
    end
    self.favCache = nil
end
--- The server's installed mods that Steam says changed after you last joined it (PZMM's servers.txt has when).
function PZMMMods:modsMovedOn(sel, ids)
    local out = {}
    if not sel.ip then return out end
    local addr = tostring(sel.ip) .. ":" .. tostring(sel.port or "16261")
    local at
    for _, e in ipairs(self:recentServers()) do if e.addr == addr or e.addr == tostring(sel.ip) then at = e.at; break end end
    if not at then return out end
    for _, id in ipairs(ids) do
        local d = self.model.mods[id]
        local st = d and self:steamOf(d)
        if st and (st.updated or 0) > at then out[#out + 1] = d.name or id end
    end
    return out
end
--- How many Workshop items joining downloads and their size, when Steam's rules for the server name the items
--- (nil size: not known); asks Steam for the details of items it hasn't been asked about.
function PZMMMods:downloadSize(sel, r)
    local items = r and r.items and tostring(r.items) or ""
    if items == "" then return 0, nil end
    local have, count, bytes, ask, known = {}, 0, 0, nil, true
    pcall(function()
        local ids2 = getSteamWorkshopItemIDs()
        for i = 0, ids2:size() - 1 do have[tostring(ids2:get(i))] = true end
    end)
    for item in items:gmatch("%d+") do
        if not have[item] then
            count = count + 1
            local st = self.steam and self.steam[item]
            if st and (st.size or 0) > 0 then
                bytes = bytes + st.size
            else
                known = false
                ask = ask or ArrayList.new()
                ask:add(item)
            end
        end
    end
    if ask and not sel.sized then
        sel.sized = true
        self.steam = self.steam or {}
        pcall(function() querySteamWorkshopItemDetails(ask, PZMMMods.onSteamDetails, self) end)
    end
    return count, known and bytes or nil
end

--- The rules Steam sent for a picked server, or nil.
function PZMMMods:rulesFor(sel)
    local c = { sel.host, sel.ip, false }
    if sel.sv then pcall(function() c[3] = sel.sv:getIp2() end) end
    for i = 1, 3 do
        local r = c[i] and PZMMMods.rulesBy[tostring(c[i]) .. ":" .. tostring(sel.port or "")]
        if r then return r end
    end
    return nil
end

--- Turn on exactly the installed mods among `ids` (and what they need), the rest off: Undo takes it back.
function PZMMMods:useMods(ids, name)
    local want, missing = {}, 0
    for _, id in ipairs(ids) do
        want[id] = true
        if not self.model.mods[id] then missing = missing + 1 end
    end
    local snap = self:snapshot()
    local ons, offs = {}, {}
    for id, d in pairs(self.model.mods) do
        if want[id] and not d.isActive then
            ons[#ons + 1] = d
        elseif not want[id] and d.isActive then
            offs[#offs + 1] = d
        end
    end
    -- where two of them clash the first goes on: the one earlier in the server's list, then by name
    local pos = {}
    for i, id in ipairs(ids) do pos[id] = pos[id] or i end
    table.sort(ons, function(a, b)
        local pa, pb = pos[a.modId] or 1e9, pos[b.modId] or 1e9
        if pa ~= pb then return pa < pb end
        return byName(a, b)
    end)
    local skipped = self:setMany(ons, offs)
    self:rememberIfChanged(snap)
    self:say(name .. "'s mods are on now" .. (missing > 0 and (": " .. missing .. (missing == 1 and " isn't" or " aren't") .. " installed.") or ".")
        .. (skipped > 0 and (" " .. self:skipNote(skipped)) or "") .. " Undo takes it back.")
end

--- Open the game's server settings screen on a hosted server (Back returns here); else the host screen.
function PZMMMods:editServerSettings(name)
    -- leaving asks about unaccepted changes first (leaveTo closes this screen)
    self:leaveTo(function() self:editServerSettingsNow(name) end)
end
function PZMMMods:editServerSettingsNow(name)
    local closed = true
    local ok = pcall(function()
        local main = MainScreen.instance
        local scr = main.serverSettingsScreen
        scr.prevScreen = {
            aboutToShow = function() end,
            setVisible = function(_, v)
                if v then
                    PZMMMods.open()
                    if PZMMMods.instance then PZMMMods.instance.page = "servers" end
                end
            end,
        }
        scr.initialSelectedSettings = name
        main.bottomPanel:setVisible(false)
        scr:aboutToShow()
        scr:setVisible(true)
    end)
    if not ok then menuClick({ internal = "COOP" }, 0, 0) end
end

--- The picked server's details: what it says about itself, its mods against yours, and what to do with it.
function PZMMMods:drawServerPanel(x, y, w, mx, my)
    local sel = self.srvSel
    local ix, iw = x + 18, w - 36
    local sv = sel.sv
    local function g(fn)
        local ok, v = pcall(fn)
        if ok then return v end
        return nil
    end
    local r = (not sel.hosted) and self:rulesFor(sel) or nil
    local facts, desc, modStr = {}, "", ""
    local function fact(label, v)
        if v ~= nil and tostring(v) ~= "" then facts[#facts + 1] = { label, tostring(v) } end
    end
    if sel.hosted then
        -- its options are read once, when it is picked
        local o = sel.opts
        if not o then
            o = self:hostedOptions(sel.hosted, { "PublicDescription", "Mods", "MaxPlayers", "Map", "DefaultPort", "Password" })
            sel.opts = o
        end
        desc, modStr = o.PublicDescription, o.Mods
        fact("Players", o.MaxPlayers ~= "" and ("up to " .. o.MaxPlayers) or nil)
        fact("Map", o.Map)
        fact("Port", o.DefaultPort)
        fact("Password", o.Password ~= "" and "yes" or "no")
    else
        desc = (r and r.description) or (sv and g(function() return sv:getDescription() end)) or ""
        modStr = (r and r.mods) or (sv and g(function() return sv:getMods() end)) or ""
        local pl = sv and (tonumber(g(function() return sv:getPlayers() end)) or 0)
        local mx2 = sv and (tonumber(g(function() return sv:getMaxPlayers() end)) or 0)
        local ping = sv and (tonumber(g(function() return sv:getPing() end)) or 0)
        local known = sv and g(function() return sv:isResponded() end)
        if not sv then
            local si = PZMMMods.srvInfo[tostring(sel.host or sel.ip) .. ":" .. tostring(sel.port or "")]
            if si then pl, mx2, ping, known = si.players, si.max, si.ping, true end
        end
        fact("Players", (pl and (known or pl > 0)) and (pl .. "/" .. tostring(mx2 or "?")) or nil)
        fact("Ping", (ping and ping > 0) and (ping .. " ms") or nil)
        fact("Version", (r and r.version) or (sv and g(function() return sv:getVersion() end)))
        fact("Map", sv and g(function() return sv:getMapName() end))
        if sv then fact("Password", g(function() return sv:isPasswordProtected() end) and "yes" or "no") end
        if sel.ip and K.favKey(self, sel) then fact("Favourite", "yes") end
        fact("Address", sel.ip .. ":" .. tostring(sel.port or ""))
        if not r then fact("Rules", "asking Steam...") end
    end
    -- the wrapped description and the parsed Mods= are kept while their text and the width stay the same
    local c = sel.panel
    modStr = tostring(modStr or "")
    if not c or c.desc ~= desc or c.iw ~= iw or c.mods ~= modStr or c.font ~= H_SMALL then
        local text2 = tostring(desc or ""):gsub("<[Ll][Ii][Nn][Ee]>", "\n"):gsub("<[Bb][Rr]%s*/?>", "\n"):gsub("<[^>]*>", " "):gsub("\\n", "\n")
        local a, b = self:modIds(modStr)
        local so, op = K.tagSides(desc)
        c = { desc = desc, iw = iw, mods = modStr, font = H_SMALL, dlines = wrap(text2, iw - 13), ids = a, missing = b, so = so, op = op }
        sel.panel = c
    end
    local dlines, ids, missing = c.dlines, c.ids, c.missing
    local stat = self:modStatuses(sel, r, ids)
    local nd = math.min(#dlines, 6)
    local miss = {}
    for _, id in ipairs(missing) do miss[id] = true end
    sel.skip = sel.skip or {}
    -- "server only" mods are left off unless ticked, as in the app (an optional one is only marked)
    if sel.sideSeen ~= c.mods then
        sel.sideSeen = c.mods
        for id in pairs(c.so) do sel.skip[id] = true end
    end
    local tab = (not sel.hosted and sel.tab) or "mods"
    -- your last join of it, from PZMM's servers.txt (the app keeps each join's setup; the game can't restore one)
    local hist
    if not sel.hosted and sel.ip then
        local addr = tostring(sel.ip) .. ":" .. tostring(sel.port or 16261)
        for _, rr in ipairs(self:recentServers()) do
            if rr.addr == addr and (rr.at or 0) > 0 then hist = rr; break end
        end
    end
    if tab == "history" and not hist then tab = "mods" end
    -- what to do: Set up and join (the ticked mods on, then join), Join as-is, Use its mods, Favourite...
    local buttons = {}
    local function button(label, kind, tipText, fn, icon) buttons[#buttons + 1] = { label, kind, tipText, fn, icon } end
    local function ticked()
        local out = {}
        for _, id in ipairs(ids) do if miss[id] or not sel.skip[id] then out[#out + 1] = id end end
        return out
    end
    if sel.join and #ids > 0 and not sel.hosted then
        button(T("launcher_set_up_join", "Set up and join"), "primary", "Set up and join\nTurns on the ticked mods you have (the rest off), then joins. The game downloads any Workshop mods it needs as it connects.", function()
            self:useMods(ticked(), sel.name ~= "" and sel.name or tostring(sel.ip))
            sel.join()
        end, TB.play)
        button(T("launcher_join_as_is", "Join as-is"), nil, "Join as-is\nJoins with your mods as they are, turning nothing on.", sel.join, TB.play)
    elseif sel.join then
        button("Join", "primary", "Join\nThe game downloads any Workshop mods it needs as it connects.", sel.join, TB.play)
    end
    if #ids > 0 then
        local nt = #ticked() - #missing
        button(T("srv_use_mods", "Use its mods"), nil, "Use its mods\nTurns on exactly the " .. nt .. " ticked of its " .. #ids .. " mods you have installed, and the rest off." .. (#missing > 0 and (" " .. #missing .. (#missing == 1 and " isn't" or " aren't") .. " installed: joining downloads Workshop ones.") or "") .. " Undo takes it back.", function()
            self:useMods(ticked(), sel.name ~= "" and sel.name or tostring(sel.ip))
        end, TB.check)
    end
    if sel.hosted then
        local stx = sel.hosted
        button("Give it my mods", nil, "Give it my mods\nIts Mods, WorkshopItems and Map settings become the mods on in My mods now.", function() self:giveServerMods(stx) end)
        button(T("srv_edit", "Edit settings"), nil, "Edit settings\nThe game's own server settings screen for it. Back brings you here.", function() self:editServerSettings(sel.name) end)
    else
        local fav = K.favKey(self, sel)
        if sel.ip and sel.ip ~= "" then
            button(fav and T("launcher_unfavourite", "Remove favourite") or T("launcher_favourite", "Favourite"), nil, "Favourite\nAdds it to the game's saved servers (a server with a password is added in the game's server browser), or takes it out.", function() self:toggleFavourite(sel) end, TB.star)
            button(T("launcher_battlemetrics", "BattleMetrics"), nil, "BattleMetrics\nIts history and player counts.", function() openLink("https://www.battlemetrics.com/servers/zomboid?q=" .. urlEncode(tostring(sel.ip))) end, TB.external)
        end
        local hasPw = false
        if fav then pcall(function() hasPw = tostring(fav:getServerPassword() or "") ~= "" end) end
        if hasPw then
            button(T("srv_forget_pw", "Forget saved password"), nil, "Forget saved password\nTakes the server's password out of the game's saved server: joining asks for it again. PZMM keeps no password of its own.", function()
                self:ask({ title = "Forget the saved password of " .. (sel.name ~= "" and sel.name or tostring(sel.ip)) .. "?", lines = { "The game's saved server keeps its address and accounts. Joining asks for the password each time." }, choices = {
                    { "Forget it", function()
                        local ok = pcall(function()
                            fav:setServerPassword("")
                            updateServerToAccountList(fav)
                        end)
                        self.favCache = nil
                        self:say(ok and "The saved password is gone." or "Couldn't change the saved server.")
                    end },
                } })
            end, TB.lock)
        end
        button("Ask Steam again", "quiet", "Ask Steam again\nAsks for its description, version and mods again.", function() self:askRules(sel) end, TB.alert)
    end
    local brows, bx0 = 1, 0
    for _, bt in ipairs(buttons) do
        local bw = width(bt[1]) + 28 + (bt[5] and 20 or 0)
        if bx0 > 0 and bx0 + bw > iw then brows, bx0 = brows + 1, 0 end
        bx0 = bx0 + bw + 8
    end
    -- warnings: this build isn't the server's, a mod moved on since you last joined
    local warns = {}
    if not sel.hosted then
        local theirs = tostring((r and r.version) or (sv and g(function() return sv:getVersion() end)) or "")
        local mine = K.myBuild()
        if theirs ~= "" and mine ~= "" and K.buildKey(theirs) ~= K.buildKey(mine) then
            warns[#warns + 1] = T("srv_build_mismatch", "This server runs %1; you play Build %2. Joining fails until you play the same build.", theirs, mine)
        end
        local moved = self:modsMovedOn(sel, ids)
        if #moved > 0 then
            warns[#warns + 1] = T("srv_moved_on", "%1 was updated on the Workshop after you last joined this server: joining may fail until the server restarts.", moved[1] .. (#moved > 1 and (" and " .. (#moved - 1) .. " more") or ""))
        end
    end
    local wlines = {}
    for _, wmsg in ipairs(warns) do for _, l in ipairs(wrap(wmsg, iw - 24)) do wlines[#wlines + 1] = l end end
    -- the Rules tab
    local rules, wipeHint = {}, nil
    if tab == "rules" and not sel.hosted then
        local function yes(v) return v == nil and "unknown" or (v and "Yes" or "No") end
        rules[#rules + 1] = { T("srv_rule_open", "Open to all"), r and yes(r.open) or "asking Steam..." }
        if r and r.public ~= nil then rules[#rules + 1] = { T("srv_rule_public", "Listed publicly"), yes(r.public) } end
        rules[#rules + 1] = { "PvP", yes(r and r.pvp) }
        rules[#rules + 1] = { T("srv_rule_version", "Version"), (r and r.version) or (sv and g(function() return sv:getVersion() end)) or "unknown" }
        rules[#rules + 1] = { T("srv_rule_mods", "Mods"), tostring(#ids) }
        local wipe = (r and r.lastWipe) or (sv and g(function() return sv:getLastWipe() end)) or 0
        if wipe and wipe > 0 then
            local days = math.floor((now() - wipe) / 86400)
            rules[#rules + 1] = { T("srv_rule_wipe", "Last wipe"), days <= 0 and "today" or days == 1 and "yesterday" or (days .. " days ago") }
            wipeHint = wrap(T("srv_wipe_hint", "Last wipe is when its world was made: it doesn't change when the server restarts."), iw)
        else
            rules[#rules + 1] = { T("srv_rule_wipe", "Last wipe"), "unknown" }
        end
    end
    -- the plan: what joining downloads
    local plan
    if tab == "mods" and not sel.hosted and #ids > 0 then
        if #missing == 0 then
            plan = T("srv_plan_nothing", "Nothing to download")
        else
            local count, bytes = self:downloadSize(sel, r)
            plan = bytes and (count .. (count == 1 and " Workshop item" or " Workshop items") .. " to download (" .. K.mb(bytes) .. ")")
                or (#missing .. (#missing == 1 and " mod" or " mods") .. " to download: the game gets them as it connects (sizes aren't known here)")
        end
    end
    local plines = plan and wrap(plan, iw) or {}
    -- under the mods: how many it doesn't list, how many you ticked off; with none, why
    local mlines = {}
    local function mline(str, col) for _, l in ipairs(wrap(str, iw)) do mlines[#mlines + 1] = { l, col } end end
    if tab == "mods" and #ids > 0 then
        if r and r.modCount and r.modCount > #ids then
            mline(T("srv_and_more", "and %1 more: the server runs %2 mods, but only the first few show outside the game", r.modCount - #ids, r.modCount), C.muted)
        end
        local nskip = 0
        for _, id in ipairs(ids) do if sel.skip[id] then nskip = nskip + 1 end end
        if nskip == 1 then
            mline(T("srv_skipped_one", "1 mod is ticked off: it isn't downloaded or put in the profile. The server may turn you away, or things from it may be missing."), C.warn)
        elseif nskip > 1 then
            mline(T("srv_skipped", "%1 mods are ticked off: they aren't downloaded or put in the profile. The server may turn you away, or things from them may be missing.", nskip), C.warn)
        end
    elseif tab == "mods" then
        local known = r ~= nil or sel.hosted or (sv and g(function() return sv:isResponded() end))
        mline(known and T("srv_no_mods", "No mods: it runs vanilla Project Zomboid.") or T("srv_mods_unknown", "Its mods show once it answers."), C.muted)
    end
    local hlines = (tab == "history" and hist) and wrap(T("srv_history_hint", "Your last join of this server, from PZMM's list. Restoring a past join's setup, and who is online, are in the PZMM app: the game can't ask a server for either."), iw) or {}
    local labelW = 0
    for _, f in ipairs(facts) do labelW = math.max(labelW, width(f[1])) end
    for _, f in ipairs(rules) do labelW = math.max(labelW, width(f[1])) end
    labelW = labelW + 14
    local colW = 260
    local cols = math.max(1, math.floor(iw / colW))
    local cap = cols * 12
    local nshow = math.min(#ids, cap)
    local rows = math.ceil(nshow / cols)
    local rowH = BOX + 4
    local tabsH = sel.hosted and 0 or (BAR + 12)
    local body = 0
    if tab == "history" then
        body = (H_SMALL + 6) + #hlines * (H_SMALL + 2) + 8
    elseif tab == "rules" then
        body = #rules * (H_SMALL + 4) + (wipeHint and (#wipeHint * (H_SMALL + 2) + 6) or 0) + 8
    elseif tab == "mods" then
        body = (#ids > 0 and (H_SMALL + 8 + rows * rowH + (#ids > cap and (H_SMALL + 4) or 0)) or 0) + #mlines * (H_SMALL + 2) + 8
    end
    local h = 18 + H_MEDIUM + 4 + H_SMALL + 14 + brows * (BAR + 8) + 6 + (#wlines > 0 and (#wlines * (H_SMALL + 2) + 10) or 0)
        + #facts * (H_SMALL + 4) + (nd > 0 and (nd * (H_SMALL + 2) + 12) or 0) + tabsH + body + (#plines > 0 and (#plines * (H_SMALL + 2) + 8) or 0) + 12
    card(self, x, y, w, h)
    local py = y + 18
    local shownTitle = fit(sel.name ~= "" and sel.name or tostring(sel.ip), iw - 100)
    bold(self, shownTitle, ix, py, C.text, MEDIUM)
    if r and r.pvp and width(shownTitle, MEDIUM) + 80 < iw - 100 then tagChip(self, "PvP", ix + width(shownTitle, MEDIUM) + 12, py + math.floor((H_MEDIUM - H_SMALL - 2) / 2), false) end
    local _, co = toolButton(self, TB.x, nil, ix + iw - BAR, py - 4, mx, my)
    if co then tip(self, "Close\nHides these details.") end
    self.feedHits[#self.feedHits + 1] = { kind = "fn", fn = function() self.srvSel = nil end, x = ix + iw - BAR, y = py - 4, w = BAR, h = BAR }
    py = py + H_MEDIUM + 4
    text(self, sel.hosted and "Your hosted server" or "Server", ix, py, C.muted)
    py = py + H_SMALL + 14
    local bx = ix
    for _, bt in ipairs(buttons) do
        local bw = width(bt[1]) + 28 + (bt[5] and 20 or 0)
        if bx > ix and bx + bw > ix + iw then bx, py = ix, py + BAR + 8 end
        local _, _, over = flatButton(self, bt[1], bx, py, mx, my, bt[2], bt[5])
        if over then tip(self, bt[3]) end
        self.feedHits[#self.feedHits + 1] = { kind = "fn", fn = bt[4], x = bx, y = py, w = bw, h = BAR }
        bx = bx + bw + 8
    end
    py = py + BAR + 14
    for _, l in ipairs(wlines) do
        if TB.alert and l == wlines[1] then self:drawTextureScaled(TB.alert, ix, py + math.floor((H_SMALL - 14) / 2), 14, 14, 1, C.warn.r, C.warn.g, C.warn.b) end
        text(self, l, ix + 20, py, C.warn)
        py = py + H_SMALL + 2
    end
    if #wlines > 0 then py = py + 10 end
    for _, f in ipairs(facts) do
        text(self, f[1], ix, py, C.muted)
        text(self, fit(f[2], iw - labelW), ix + labelW, py, C.text)
        py = py + H_SMALL + 4
    end
    if nd > 0 then
        py = py + 4
        -- a 3 px line down its left, the text set in from it
        rect(self, ix, py, 3, nd * (H_SMALL + 2) - 2, C.line, 1)
        for i = 1, nd do
            text(self, dlines[i], ix + 13, py, C.desc)
            py = py + H_SMALL + 2
        end
        py = py + 8
    end
    -- Mods | Rules
    if not sel.hosted then
        local tabs = { { "mods", T("srv_tab_mods", "Mods") }, { "rules", T("srv_tab_rules", "Rules") } }
        if hist then tabs[#tabs + 1] = { "history", T("srv_tab_history", "History") } end
        local sw = 0
        for _, t in ipairs(tabs) do sw = sw + width(t[2]) + 24 end
        local tx = ix
        for i, t in ipairs(tabs) do
            local tw = width(t[2]) + 24
            local on = tab == t[1]
            if on then roundEnds(self, tx, py, tw, BAR, 6, C.accent, 0.32, i == 1, i == #tabs) end
            local over = mx >= tx and mx < tx + tw and my >= py and my < py + BAR
            local f = on and bold or text
            f(self, t[2], tx + 12, py + (BAR - H_SMALL) / 2, on and C.onSelected or over and C.text or C.muted)
            local id = t[1]
            self.feedHits[#self.feedHits + 1] = { kind = "fn", fn = function() sel.tab = id end, x = tx, y = py, w = tw, h = BAR }
            tx = tx + tw
        end
        outline(self, ix, py, sw, BAR, 6, C.line)
        py = py + BAR + 12
    end
    if tab == "history" then
        local okd, ds = pcall(os.date, "%d %b %Y, %H:%M", K.LT(hist.at))
        text(self, (okd and ds or "") .. "  " .. K.ago(hist.at), ix, py, C.text)
        py = py + H_SMALL + 6
        for _, l in ipairs(hlines) do
            text(self, l, ix, py, C.muted)
            py = py + H_SMALL + 2
        end
    elseif tab == "rules" then
        for _, f in ipairs(rules) do
            text(self, f[1], ix, py, C.muted)
            text(self, fit(tostring(f[2]), iw - labelW), ix + labelW, py, C.text)
            py = py + H_SMALL + 4
        end
        if wipeHint then
            py = py + 6
            for _, l in ipairs(wipeHint) do
                text(self, l, ix, py, C.muted)
                py = py + H_SMALL + 2
            end
        end
    elseif tab == "mods" then
      if #ids > 0 then
        local label = "Mods: " .. #ids .. (#missing > 0 and ("   " .. #missing .. " not installed") or "   you have them all")
        bold(self, label, ix, py, #missing > 0 and C.warn or C.ok)
        py = py + H_SMALL + 8
        for i = 1, nshow do
            local col, row = (i - 1) % cols, math.floor((i - 1) / cols)
            local id = ids[i]
            local d = self.model.mods[id]
            local cx = ix + col * colW
            local ry = py + row * rowH
            local label2 = d and d.name or id
            -- its state as a chip at the column's right: missing, on, off
            local sk = stat[id]
            local st = sk == "installed" and T("srv_st_installed", "Installed") or sk == "found" and T("srv_st_found", "On the Workshop")
                or sk == "likely" and T("srv_st_likely", "Probably") or sk == "unknown" and T("srv_st_unknown", "Not found") or "missing"
            local stc
            if sk == "installed" then stc = d.isActive and C.ok or nil elseif sk == "found" then stc = C.ok else stc = C.warn end
            local chipW = width(st) + 12
            local lx = cx
            if d then
                -- a tick says it goes on when you Set up (an unticked one is turned off)
                local on = not sel.skip[id]
                checkbox(self, cx, ry + 2, on)
                self.feedHits[#self.feedHits + 1] = { kind = "fn", fn = function() sel.skip[id] = on or nil end, x = cx - 4, y = ry - 2, w = BOX + 8, h = rowH }
                if mx >= cx - 4 and mx < cx + BOX + 4 and my >= ry - 2 and my < ry - 2 + rowH then tip(self, "Use " .. label2 .. "\nTicked: Set up and join turns it on. Unticked: it's turned off.") end
                lx = cx + BOX + 8
            end
            local shownName = fit(label2, colW - 24 - chipW - 12 - (lx - cx))
            text(self, shownName, lx, ry + math.floor((rowH - H_SMALL) / 2), miss[id] and C.warn or C.text)
            local side = c.so[id] and T("srv_server_only", "Server only") or c.op[id] and T("srv_optional", "Optional") or nil
            if side then
                local sx = lx + width(shownName) + 8
                if sx + width(side) + 12 < cx + colW - 24 - chipW - 8 then chip(self, side, sx, ry + math.floor((rowH - H_SMALL - 2) / 2), nil, nil, C.line) end
            end
            chip(self, st, cx + colW - 24 - chipW, ry + math.floor((rowH - H_SMALL - 2) / 2), stc, nil, C.line)
        end
        py = py + rows * rowH
        if #ids > cap then text(self, "+" .. (#ids - cap) .. " more", ix, py, C.muted) end
        py = py + (#ids > cap and (H_SMALL + 4) or 0) + 8
      end
      for _, ml in ipairs(mlines) do
        text(self, ml[1], ix, py, ml[2])
        py = py + H_SMALL + 2
      end
    end
    for _, l in ipairs(plines) do
        text(self, l, ix, py, #missing > 0 and C.warn or C.muted)
        py = py + H_SMALL + 2
    end
    return y + h + 20
end


--- Join a server straight away, through the game's own connecting screen: the account you type (a new name
--- makes a new account), and the server's password if it has one.
function PZMMMods:joinServer(name, ip, port, protected, user, pass)
    local function go(u, pw, spw)
        self.lastUser = u
        self:beforeJoin(function(prev)
            pcall(function() ConnectToServer.instance:connect(prev, name or "", u, pw or "", ip, "", tostring(port), spw or "", false, true, 1) end)
        end)
    end
    if user then return go(user, pass, "") end
    self:prompt("Your name on " .. (name ~= "" and name or ip), self.lastUser or "", "Next", function(u)
        u = tostring(u or ""):gsub("^%s+", ""):gsub("%s+$", "")
        if u == "" then return end
        self:prompt("Account password (blank for a new account)", "", protected and "Next" or "Join", function(pw)
            if not protected then return go(u, pw, "") end
            self:prompt("The server's password", "", "Join", function(spw) go(u, pw, spw) end, true)
        end, true)
    end)
end

--- The Internet list's filters and sort, as the app's: players, password, PvP (when Steam's rules say), build, mods,
--- not full, not empty. Each is a menu or a toggle; they work over the rows already asked for. Returns the y under them.
function PZMMMods:drawServerFilters(x, y, w, mx, my)
    local f = self.sf
    if not f then f = {}; self.sf = f end
    local function set(k, v)
        f[k] = v
        self.sfVer = (self.sfVer or 0) + 1
        self.srvCache = nil
    end
    -- the builds the servers report, once per list
    local b = self.srvBuilds
    if not b or b.n ~= #PZMMMods.inet or b.k ~= PZMMMods.srvN then
        local seen, list = {}, {}
        for _, sv in ipairs(PZMMMods.inet) do
            local v = ""
            pcall(function() v = tostring(sv:getVersion() or "") end)
            if v ~= "" and not seen[v] then seen[v] = true; list[#list + 1] = v end
        end
        for _, i2 in pairs(PZMMMods.srvInfo) do
            local v = i2.ver or ""
            if v ~= "" and not seen[v] then seen[v] = true; list[#list + 1] = v end
        end
        table.sort(list)
        b = { n = #PZMMMods.inet, k = PZMMMods.srvN, list = list }
        self.srvBuilds = b
    end
    local function menu(key, title, opts)
        local cur
        for _, o in ipairs(opts) do if f[key] == o[2] then cur = o[1] end end
        local on = f[key] ~= nil
        return { title = on and (title .. ": " .. tostring(cur)) or title, on = on, tip = title .. "\nFilter the list.", fn = function(mx2, my2)
            local items = {}
            for _, o in ipairs(opts) do
                local v = o[2]
                items[#items + 1] = { o[1], function() set(key, v) end, f[key] == v }
            end
            self:menu(items, mx2, my2)
        end }
    end
    local builds = { { T("srv_any_build", "Any build"), nil } }
    for _, v in ipairs(b.list) do builds[#builds + 1] = { v, v } end
    local sorts = { { "players", T("srv_by_players", "Most players") }, { "ping", T("srv_by_ping", "Lowest ping") }, { "name", T("srv_by_name", "Name") } }
    local sortName = sorts[1][2]
    for _, s in ipairs(sorts) do if s[1] == (self.srvSort or "players") then sortName = s[2] end end
    local ctl = {
        menu("players", T("srv_players", "Players"), { { T("srv_any_players", "Any players"), nil }, { "1 or more", 1 }, { "5 or more", 5 }, { "10 or more", 10 }, { "20 or more", 20 } }),
        menu("pw", T("srv_password", "Password"), { { T("srv_any_password", "Password or not"), nil }, { T("srv_no_password", "No password"), "no" }, { T("srv_with_password", "With a password"), "yes" } }),
        menu("pvp", "PvP", { { T("srv_any_pvp", "PvP or PvE"), nil }, { "PvP", true }, { "PvE", false } }),
        menu("build", T("srv_build", "Build"), builds),
        menu("mods", T("srv_mods", "Mods"), { { T("srv_any_mods", "Any mods"), nil }, { T("srv_only_mine", "Only mods I have"), "have" }, { "Missing some", "missing" } }),
        menu("maxdl", T("srv_max_download", "To download"), { { T("srv_any_download", "Any number to download"), nil }, { "At most 0 to download", 0 }, { "At most 1 to download", 1 }, { "At most 3 to download", 3 }, { "At most 5 to download", 5 }, { "At most 10 to download", 10 } }),
        { title = f.map and ("Map: " .. f.map) or T("srv_map", "Map"), on = f.map ~= nil, tip = "Map\nOnly servers whose map name has these letters. Click again to change or clear it.", fn = function()
            self:prompt("Part of a map name (empty: any)", f.map or "", "Filter", function(t)
                t = tostring(t or ""):gsub("^%s+", ""):gsub("%s+$", "")
                set("map", t ~= "" and t:lower() or nil)
            end)
        end },
        { title = T("srv_not_full", "Not full"), on = f.notFull == true, tip = "Not full\nHides servers with no free slot.", fn = function() set("notFull", not f.notFull or nil) end },
        { title = T("srv_not_empty", "Not empty"), on = f.notEmpty == true, tip = "Not empty\nHides servers nobody is on.", fn = function() set("notEmpty", not f.notEmpty or nil) end },
        { title = sortName, icon = TB.sort, tip = "Sort\nHow the list is ordered.", fn = function(mx2, my2)
            local items = {}
            for _, s in ipairs(sorts) do
                items[#items + 1] = { s[2], function() self.srvSort = s[1]; self.sfVer = (self.sfVer or 0) + 1; self.srvCache = nil end, (self.srvSort or "players") == s[1] }
            end
            self:menu(items, mx2, my2)
        end },
    }
    local any = false
    for _, v in pairs(f) do if v ~= nil then any = true end end
    if any then
        ctl[#ctl + 1] = { title = T("tool_clear", "Clear filters"), icon = TB.x, tip = "Clear filters\nShow every server again.", fn = function()
            self.sf = {}
            self.sfVer = (self.sfVer or 0) + 1
            self.srvCache = nil
        end }
    end
    local bx = x
    for _, c in ipairs(ctl) do
        local bw = width(c.title) + 46
        if bx + bw > x + w and bx > x then bx, y = x, y + BAR + 6 end
        local _, over = toolButton(self, c.icon, c.title, bx, y, mx, my, c.on)
        if over then tip(self, c.tip) end
        local fn = c.fn
        self.feedHits[#self.feedHits + 1] = { kind = "fn", fn = function() fn(mx, my) end, x = bx, y = y, w = bw, h = BAR }
        bx = bx + bw + 6
    end
    y = y + BAR
    if f.pvp ~= nil then
        y = y + 4
        text(self, "PvP is known only for servers Steam has sent the rules of: pick a server to ask.", x, y, C.muted)
        y = y + H_SMALL
    end
    return y + 16
end

--- Servers: find servers on the Internet with each one's mods checked against yours, join by address, your
--- saved servers, the ones you joined lately (from PZMM), and your hosted servers, which take your mods.
function PZMMMods:drawServers(x, y, w, mx, my, top, bh)
    y = self:drawNotice(x, y, w, T("notice_servers", "running a dedicated server (setup, start, stop, console, players, backups and files), hosting beyond start and stop (restart, a warned restart with abort, updating, adopting or finding an existing server, checking the Workshop for external servers, copying diagnostics), hosting tabs (automation with crash and scheduled restarts, warnings, backups every N and the Discord bot and relay; world tools to copy a world, reset chunks or migrate; a mods editor that adds from a collection, profile or preset, autosorts and checks conflicts; the ini settings editor with presets), remote servers (RCON, FTP and the Files tab), the public join address and opening pzmm:// links, a profile per server, the country filter and its setting, listing servers without Steam (a Steam Web API key), your Steam friends' servers, a server's player list, restoring a past join's setup and, in the join dialog, map overlap and maps-not-in-mods warnings, per-mod size and updated date, server keywords, a caution for a very recent update and Remember password (the game can't ask a server who is online)."), mx, my)
    local bx = x
    local function button(label, kind, tipText, fn, icon)
        local bw, _, over = flatButton(self, label, bx, y, mx, my, kind, icon)
        if over then tip(self, tipText) end
        self.feedHits[#self.feedHits + 1] = { kind = "fn", fn = fn, x = bx, y = y, w = bw, h = BAR }
        bx = bx + bw + 8
    end
    -- drawing runs every frame: the rows, their mod ids and servers.txt are worked out once per visit (a
    -- gap in drawing means the page was left), and again when the list or the filter changes
    local t = getTimestampMs()
    if not self.srvAt or t - self.srvAt > 400 then self.recent, self.srvCache, self.hostedRow = nil, nil, nil end
    self.srvAt = t
    local steam = false
    pcall(function() steam = getSteamModeActive() end)
    if steam then
        button(#PZMMMods.inet > 0 and "Find again" or "Find servers", "primary", "Find servers\nAsks Steam for the Internet list. Each server's mods are checked against yours.", function()
            PZMMMods.inet = {}
            PZMMMods.inetAt = getTimestampMs()
            self.recent, self.srvCache = nil, nil
            listen()
            PZMMMods.askInet()
        end, TB.search)
    end
    button("Look up a server", nil, "Look up a server\nA server's address and port, as ip:port: its rules and mods show first, then you can join.", function()
        self:prompt("Server address (ip:port)", "", "Next", function(addr)
            local ip, port = tostring(addr or ""):match("^%s*([^%s:]+):?(%d*)%s*$")
            if not ip then return self:say("Type the server's address, like 203.0.113.5:16261.") end
            port = port ~= "" and port or "16261"
            -- looked up first: its rules, mods and players show here, and Join is a button on them
            self:selectServer({ name = "", ip = ip, port = port, join = function() self:joinServer("", ip, port, true) end })
        end)
    end)
    bx = bx + K.searchSpot(self, bx, y, 200, top, bh) + 8
    button(T("srv_host", "Host"), "quiet", "Host\nThe game's own host screen: start a server on this PC and play on it.", function() self:gameScreen("COOP") end)
    button("Game's server browser", "quiet", "Game's server browser\nThe game's own screen: favourites, accounts and the Internet list.", function() self:gameScreen("MULTIPLAYER") end)
    y = y + BAR + 20
    y = self:drawServerFilters(x, y, w, mx, my)
    if self.srvSel then y = self:drawServerPanel(x, y, w, mx, my) end

    -- the Internet list: most players first, each with how many of its mods you have
    if #PZMMMods.inet > 0 then
        local c = self.srvCache
        if not c or c.n ~= #PZMMMods.inet or c.q ~= self.serverFilter or c.v ~= self.sfVer then
            local f = self.sf or {}
            local q = self.serverFilter and self.serverFilter:lower()
            local rows, info, mods = {}, {}, {}
            for _, sv in ipairs(PZMMMods.inet) do
                local i = { name = "", pl = 0, max = 0, ping = 0, pw = false, ver = "" }
                pcall(function()
                    i.name = tostring(sv:getName() or "")
                    i.pl, i.max, i.ping = tonumber(sv:getPlayers()) or 0, tonumber(sv:getMaxPlayers()) or 0, tonumber(sv:getPing()) or 0
                    i.pw = sv:isPasswordProtected() and true or false
                    i.ver = tostring(sv:getVersion() or "")
                    i.mapFull = tostring(sv:getMapName() or "")
                    i.map = i.mapFull:match("^[^;]+") or ""
                end)
                local ok = not q or i.name:lower():find(q, 1, true) ~= nil
                ok = ok and (not f.players or i.pl >= f.players)
                ok = ok and (f.pw == nil or (f.pw == "yes") == i.pw)
                ok = ok and (not f.build or i.ver == f.build)
                ok = ok and (not f.notFull or i.max == 0 or i.pl < i.max)
                ok = ok and (not f.notEmpty or i.pl > 0)
                ok = ok and (not f.map or (i.mapFull or ""):lower():find(f.map, 1, true) ~= nil)
                if ok and f.pvp ~= nil then
                    local ip, port = "", 16261
                    pcall(function() ip, port = sv:getIp() or "", sv:getPort() or 16261 end)
                    local r = PZMMMods.rulesBy[ip .. ":" .. tostring(port)]
                    ok = r ~= nil and r.pvp ~= nil and r.pvp == f.pvp
                end
                if ok and (f.mods or f.maxdl) then
                    local a, b = self:serverMods(sv)
                    mods[sv] = { a, b }
                    if f.mods then ok = (f.mods == "have" and #b == 0) or (f.mods == "missing" and #b > 0) end
                    if ok and f.maxdl then ok = #b <= f.maxdl end
                end
                if ok then
                    rows[#rows + 1] = sv
                    info[sv] = i
                end
            end
            local by = self.srvSort or "players"
            table.sort(rows, function(a, b)
                local x2, y2 = info[a], info[b]
                if by == "ping" then
                    local pa, pb = x2.ping > 0 and x2.ping or 1e9, y2.ping > 0 and y2.ping or 1e9
                    if pa ~= pb then return pa < pb end
                elseif by == "name" then
                    local na, nb = x2.name:lower(), y2.name:lower()
                    if na ~= nb then return na < nb end
                elseif x2.pl ~= y2.pl then
                    return x2.pl > y2.pl
                end
                return x2.name:lower() < y2.name:lower()
            end)
            c = { n = #PZMMMods.inet, q = self.serverFilter, v = self.sfVer, rows = rows, mods = mods, info = info }
            self.srvCache = c
        end
        local rows = c.rows
        local picked = self.srvSel and self.srvSel.key
        bold(self, "Internet: " .. #rows .. " servers", x, y, C.text)
        y = y + H_SMALL + 10
        local n = math.min(200, #rows)
        if n == 0 then
            text(self, T("srv_no_match", "No server matches these filters."), x, y, C.muted)
            y = y + H_SMALL + 4
        end
        local rh = H_SMALL * 2 + 28
        if n > 0 then card(self, x, y, w, n * rh) end
        for i = 1, n do
            local sv = rows[i]
            local ry = y + (i - 1) * rh
            -- only the rows in view are drawn (the list can run to 200)
            if ry + rh >= top and ry <= top + bh then
            local name, ip, port, pl, max, ping, protected = "", "", 0, 0, 0, 0, false
            pcall(function()
                name, ip, port = sv:getName() or "", sv:getIp() or "", sv:getPort() or 16261
                pl, max, ping = tonumber(sv:getPlayers()) or 0, tonumber(sv:getMaxPlayers()) or 0, tonumber(sv:getPing()) or 0
                protected = sv:isPasswordProtected()
            end)
            listRow(self, x, ry, w, rh, i == n, mx, my, picked == ip .. ":" .. tostring(port))
            local jw = width("Join") + 48
            local jx, jy = x + w - 16 - jw, ry + (rh - BAR) / 2
            local _, _, o = flatButton(self, "Join", jx, jy, mx, my, "primary", TB.play)
            if o then tip(self, "Join\nThe game downloads any Workshop mods it needs as it connects.") end
            local nm, ipx, px, pr = name, ip, port, protected
            self.feedHits[#self.feedHits + 1] = { kind = "fn", fn = function() self:joinServer(nm, ipx, px, pr) end, x = jx, y = jy, w = jw, h = BAR }
            local svx = sv
            local tx = x + 16
            -- the star: a favourite (a saved server of the game's), click to add or remove
            local isFav = K.favs(self)[tostring(ip) .. ":" .. tostring(port)] ~= nil
            local star = isFav and K.STAR_ON or K.STAR_OFF
            local sc = isFav and C.accentText or C.muted
            if star then self:drawTextureScaled(star, tx, ry + 12 + math.floor((H_SMALL - 14) / 2), 14, 14, 1, sc.r, sc.g, sc.b) end
            if mx >= tx - 4 and mx < tx + 20 and my >= ry and my < ry + 32 then tip(self, isFav and "Favourite\nClick to take it out of your saved servers." or "Favourite\nClick to add it to your saved servers.") end
            self.feedHits[#self.feedHits + 1] = { kind = "fn", fn = function() self:toggleFavourite({ sv = svx, name = nm, ip = ipx, port = px }) end, x = tx - 4, y = ry, w = 24, h = 32 }
            -- (the star's hit comes before the row's: the first hit wins)
            self.feedHits[#self.feedHits + 1] = { kind = "fn", fn = function()
                self:selectServer({ name = nm, ip = ipx, port = px, sv = svx, join = function() self:joinServer(nm, ipx, px, pr) end })
            end, x = x, y = ry, w = w, h = rh }
            tx = tx + 22
            local shownName = fit(name ~= "" and name or ip, jx - 24 - tx - (protected and 20 or 0))

            bold(self, shownName, tx, ry + 12, C.text)
            if protected and TB.lock then
                self:drawTextureScaled(TB.lock, tx + width(shownName) + 8, ry + 12 + math.floor((H_SMALL - 12) / 2), 12, 12, 1, C.muted.r, C.muted.g, C.muted.b)
            end
            -- the PvP tag, when Steam's rules for it say so
            local rr = PZMMMods.rulesBy[tostring(ip) .. ":" .. tostring(port)]
            if rr and rr.pvp then
                local px0 = tx + width(shownName) + (protected and 28 or 10)
                if px0 + width("PvP") + 17 < jx - 24 then tagChip(self, "PvP", px0, ry + 11, false) end
            end
            local si = c.info[sv] or {}
            local meta = tostring(pl) .. "/" .. tostring(max) .. " players    " .. tostring(ping) .. " ms    " .. ((si.map or "") ~= "" and (si.map .. "    ") or "") .. ((si.ver or "") ~= "" and ("build " .. si.ver .. "    ") or "") .. ip .. ":" .. tostring(port)
            text(self, fit(meta, jx - 24 - tx - 220), tx, ry + 16 + H_SMALL, C.muted)
            local sm = c.mods[sv]
            if not sm then
                local a, b = self:serverMods(sv)
                sm = { a, b }
                c.mods[sv] = sm
            end
            local ids, missing = sm[1], sm[2]
            local cx = tx + math.min(width(meta), jx - 24 - tx - 220) + 14
            local cy = ry + 16 + H_SMALL - 1
            if #ids > 0 then
                local label = #missing == 0 and ("All " .. #ids .. " mods installed") or (#missing .. " of " .. #ids .. " mods missing")
                local right = chip(self, label, cx, cy, #missing == 0 and C.ok or C.warn)
                if mx >= cx and mx < right and my >= cy and my < cy + H_SMALL + 2 then
                    tip(self, #missing == 0 and "Its mods\nYou have all of them." or ("Mods you don't have\n" .. table.concat(missing, ", ") .. "\nJoining downloads Workshop ones."))
                end
            end
            end
        end
        y = y + n * rh + 20
        if #rows > n then
            text(self, T("srv_more", "Showing the first %1: narrow the filters to see the rest.", n), x, y - 12, C.muted)
            y = y + H_SMALL + 8
        end
    else
        -- no list yet: Steam isn't running, it hasn't answered, or it listed nothing
        local msg
        if not steam then
            msg = T("srv_start_steam", "Start Steam and refresh: the Internet list comes from it.")
        elseif PZMMMods.inetAt and t - PZMMMods.inetAt > 20000 then
            msg = T("srv_listed_none", "Steam listed no servers. Try Find again.")
        elseif PZMMMods.inetAt then
            msg = T("srv_getting_list", "Getting the server list from Steam...")
        end
        if msg then
            text(self, fit(msg, w), x, y, C.muted)
            y = y + H_SMALL + 20
        end
    end

    -- your saved servers (the game's favourites): Join uses the account saved with it
    bold(self, T("srv_favourites", "Favourites"), x, y, C.text)
    y = y + H_SMALL + 10
    local servers = {}
    pcall(function() servers = getServerList() or {} end)
    local nSaved = #servers
    do
        local keep = {}
        for _, sv in ipairs(servers) do
            local nm, ip, port = "", "", 16261
            pcall(function() nm, ip, port = sv:getName() or "", sv:getIp() or "", sv:getPort() or 16261 end)
            if self:rowPasses(nm, ip, port, sv) then keep[#keep + 1] = sv end
        end
        servers = keep
    end
    if #servers == 0 then
        text(self, nSaved > 0 and T("srv_no_match", "No server matches these filters.") or "No saved servers yet: add one in the game's server browser.", x, y, C.muted)
        y = y + H_SMALL + 20
    else
        local rh = H_SMALL * 2 + 28
        card(self, x, y, w, #servers * rh)
        for i, sv in ipairs(servers) do
            local ry = y + (i - 1) * rh
            local name, ip, port, accounts = "", "", 16261, {}
            pcall(function()
                name, ip, port = sv:getName() or "", sv:getIp() or "", sv:getPort() or 16261
                local a = sv:getAccounts()
                for j = 0, a:size() - 1 do accounts[#accounts + 1] = a:get(j) end
            end)
            listRow(self, x, ry, w, rh, i == #servers, mx, my, self.srvSel and self.srvSel.key == ip .. ":" .. tostring(port))
            if K.STAR_ON then self:drawTextureScaled(K.STAR_ON, x + 16, ry + 12 + math.floor((H_SMALL - 14) / 2), 14, 14, 1, C.accentText.r, C.accentText.g, C.accentText.b) end
            local starHit = { kind = "fn", fn = function() self:toggleFavourite({ sv = sv, name = name, ip = ip, port = port }) end, x = x + 12, y = ry, w = 24, h = 32 }
            if mx >= x + 12 and mx < x + 36 and my >= ry and my < ry + 32 then tip(self, "Favourite\nClick to take it out of your saved servers.") end
            bold(self, name ~= "" and name or (ip .. ":" .. tostring(port)), x + 38, ry + 12, C.text)
            local meta2 = ip .. ":" .. tostring(port) .. "    " .. #accounts .. (#accounts == 1 and " account" or " accounts")
            text(self, meta2, x + 38, ry + 16 + H_SMALL, C.muted)
            do
                local modStr = ""
                pcall(function() modStr = sv:getMods() or "" end)
                local a, b = self:modIds(modStr)
                self:srvChips(self:srvInfoFor(ip, port), self:rulesFor({ ip = ip, port = port, host = hostOf(ip) }), { a, b },
                    x + 38 + width(meta2) + 14, ry + 16 + H_SMALL - 1, x + w - 16 - (width("Join") + 48) - 12, mx, my)
            end
            self.feedHits[#self.feedHits + 1] = starHit
            local jw = width("Join") + 48
            local jx, jy = x + w - 16 - jw, ry + (rh - BAR) / 2
            local _, _, o = flatButton(self, "Join", jx, jy, mx, my, nil, TB.play)
            if o then tip(self, #accounts == 1 and "Join\nWith its saved account." or "Join\nPick the account in the game's server browser.") end
            local nm, ipx, px, svx = name, ip, port, sv
            self.feedHits[#self.feedHits + 1] = { kind = "fn", fn = function() self:joinSaved(svx) end, x = jx, y = jy, w = jw, h = BAR }
            self.feedHits[#self.feedHits + 1] = { kind = "fn", fn = function()
                self:selectServer({ name = nm, ip = ipx, port = px, sv = svx, join = function() self:joinSaved(svx) end })
            end, x = x, y = ry, w = w, h = rh }
        end
        y = y + #servers * rh + 20
    end

    -- lately joined, as PZMM noted them
    local recent = {}
    for _, e in ipairs(self:recentServers()) do
        local ip, port = e.addr:match("^(.*):(%d+)$")
        if not ip then ip, port = e.addr, "16261" end
        if self:rowPasses(e.name ~= "" and e.name or e.addr, ip, port, nil) then recent[#recent + 1] = e end
    end
    if #recent > 0 then
        bold(self, "Joined lately", x, y, C.text)
        y = y + H_SMALL + 10
        local rh = H_SMALL * 2 + 28
        card(self, x, y, w, #recent * rh)
        for i, e in ipairs(recent) do
            local ry = y + (i - 1) * rh
            -- host and port split at the LAST ':' followed by digits; no port is 16261
            local ip, port = e.addr:match("^(.*):(%d+)$")
            if not ip then ip, port = e.addr, "" end
            listRow(self, x, ry, w, rh, i == #recent, mx, my, self.srvSel and self.srvSel.key == ip .. ":" .. (port ~= "" and port or "16261"))
            bold(self, e.name ~= "" and e.name or e.addr, x + 16, ry + 12, C.text)
            local days = math.floor((now() - e.at) / 86400)
            local meta2 = e.addr .. "    " .. (days <= 0 and "today" or days == 1 and "yesterday" or (days .. " days ago"))
            text(self, meta2, x + 16, ry + 16 + H_SMALL, C.muted)
            do
                local pt = port ~= "" and port or "16261"
                local rr = self:rulesFor({ ip = ip, port = pt, host = hostOf(ip) })
                local mods2
                if rr and rr.mods then
                    local a, b = self:modIds(rr.mods)
                    mods2 = { a, b }
                end
                self:srvChips(self:srvInfoFor(ip, pt), rr, mods2, x + 16 + width(meta2) + 14, ry + 16 + H_SMALL - 1, x + w - 16 - (width("Join") + 48) - 12, mx, my)
            end
            local jw = width("Join") + 48
            local jx, jy = x + w - 16 - jw, ry + (rh - BAR) / 2
            flatButton(self, "Join", jx, jy, mx, my, nil, TB.play)
            local nm = e.name
            self.feedHits[#self.feedHits + 1] = { kind = "fn", fn = function() self:joinServer(nm, ip, port ~= "" and port or "16261", true) end, x = jx, y = jy, w = jw, h = BAR }
            self.feedHits[#self.feedHits + 1] = { kind = "fn", fn = function()
                self:selectServer({ name = nm, ip = ip, port = port ~= "" and port or "16261", join = function() self:joinServer(nm, ip, port ~= "" and port or "16261", true) end })
            end, x = x, y = ry, w = w, h = rh }
        end
        y = y + #recent * rh + 20
    end

    -- your hosted servers (Zomboid/Server/*.ini): give one your mods and maps
    local hosted = {}
    self.hostedRow = self.hostedRow or {}
    for _, st in ipairs(self:hostedList()) do
        local nm = ""
        pcall(function() nm = st:getName() end)
        local o = self.hostedRow[nm]
        if not o then
            o = self:hostedOptions(st, { "DefaultPort", "Mods" })
            self.hostedRow[nm] = o
        end
        if self:rowPasses(nm, "127.0.0.1", tonumber(o.DefaultPort) or 16261, nil, o.Mods) then hosted[#hosted + 1] = st end
    end
    if #hosted > 0 then
        bold(self, "Your hosted servers", x, y, C.text)
        y = y + H_SMALL + 10
        local rh = ROW
        card(self, x, y, w, #hosted * rh)
        for i, st in ipairs(hosted) do
            local ry = y + (i - 1) * rh
            local name = ""
            pcall(function() name = st:getName() end)
            listRow(self, x, ry, w, rh, i == #hosted, mx, my, self.srvSel and self.srvSel.key == "hosted:" .. name)
            bold(self, name, x + 16, ry + (rh - H_SMALL) / 2, C.text)
            local label = "Give it my mods"
            local gw = width(label) + 28
            local gx, gy = x + w - 16 - gw, ry + (rh - BAR) / 2
            do
                local ho = self.hostedRow[name] or {}
                local hp = tonumber(ho.DefaultPort) or 16261
                self:srvChips(self:srvInfoFor("127.0.0.1", hp), nil, nil, x + 16 + width(name) + 14, ry + math.floor((rh - H_SMALL - 2) / 2),
                    gx - 8 - (width(T("srv_edit", "Edit settings")) + 28) - 12, mx, my)
            end
            local _, _, o = flatButton(self, label, gx, gy, mx, my)
            if o then tip(self, "Give it my mods\nIts Mods, WorkshopItems and Map settings become the mods on in My mods now, in load order, and your map order.") end
            local stx = st
            self.feedHits[#self.feedHits + 1] = { kind = "fn", fn = function() self:giveServerMods(stx) end, x = gx, y = gy, w = gw, h = BAR }
            local el = T("srv_edit", "Edit settings")
            local ew = width(el) + 28
            local ex = gx - 8 - ew
            local _, _, eo = flatButton(self, el, ex, gy, mx, my)
            if eo then tip(self, "Edit settings\nThe game's own server settings screen for it. Back brings you here.") end
            local nmx = name
            self.feedHits[#self.feedHits + 1] = { kind = "fn", fn = function() self:editServerSettings(nmx) end, x = ex, y = gy, w = ew, h = BAR }
            self.feedHits[#self.feedHits + 1] = { kind = "fn", fn = function()
                self:selectServer({ name = nmx, hosted = stx, key = "hosted:" .. nmx })
            end, x = x, y = ry, w = w, h = rh }
        end
        y = y + #hosted * rh
    end
    return y
end

--- A hosted server's Mods=, WorkshopItems= and Map= become what's on in My mods (asks first).
--- This server's leave-out list: the app's (leaveout.txt) when the app is installed, else the game's own copy.
function PZMMMods:leaveOutFor(name)
    local key = tostring(name):lower()
    local e = self.app and K.readLeaveOut("pzmm/leaveout.txt")[key] or K.readLeaveOut("pzmm/leaveout_game.txt")[key]
    return e and e.ids or {}
end
--- Keep a server's leave-out list: with the app, it saves it (quiet request "leaveout"); without, the game's copy, sent
--- to the app once it's there (see sendLeaveOuts).
function PZMMMods:saveLeaveOut(name, left)
    local ids = {}
    for id in pairs(left) do ids[#ids + 1] = id end
    table.sort(ids)
    local all = K.readLeaveOut("pzmm/leaveout_game.txt")
    if self.app then
        self:request("leaveout\t" .. name .. "\t" .. table.concat(ids, ";"), nil, true)
        all[name:lower()] = nil
    else
        all[name:lower()] = #ids > 0 and { name = name, ids = left } or nil
    end
    pcall(function()
        local w = getFileWriter("pzmm/leaveout_game.txt", true, false)
        for _, e in pairs(all) do
            local list = {}
            for id in pairs(e.ids) do list[#list + 1] = id end
            table.sort(list)
            w:write(e.name .. "\t" .. table.concat(list, ";") .. "\n")
        end
        w:close()
    end)
end
--- Lists the game kept while the app wasn't there go to the app once it is (as sections do), then the copy goes.
function PZMMMods:sendLeaveOuts()
    if not self.app then return end
    local all = K.readLeaveOut("pzmm/leaveout_game.txt")
    local any = false
    for _, e in pairs(all) do
        any = true
        local list = {}
        for id in pairs(e.ids) do list[#list + 1] = id end
        table.sort(list)
        self:request("leaveout\t" .. e.name .. "\t" .. table.concat(list, ";"), nil, true)
    end
    if any then pcall(function() local w = getFileWriter("pzmm/leaveout_game.txt", true, false); w:close() end) end
end

function PZMMMods:giveServerMods(st)
    local name = ""
    pcall(function() name = st:getName() end)
    local mods = self.model.mods
    local function reqs(id)
        local d, out = mods[id], {}
        if not d then return out end
        for q in pairs(d.requireMods or {}) do out[#out + 1] = q end
        for _, q in ipairs(rules(d).requires) do out[#out + 1] = q end
        return out
    end
    local order = {}
    for _, id in ipairs(self.order or {}) do if mods[id] then order[#order + 1] = id end end
    local left = self:leaveOutFor(name)
    -- the checklist: every mod ticked unless this server leaves it out; Viewpoint/ZombieBuddy add-ons labelled only
    local checks = {}
    for _, id in ipairs(order) do
        local side = K.clientSide(id, reqs)
        checks[#checks + 1] = { id = id, label = mods[id].name or id, on = not left[id],
            note = side and T("leaveout_client", "client-side add-on?") or nil,
            tip = side and (T("leaveout_client", "client-side add-on?") .. "\n" .. T("leaveout_client_tip", "It needs Project Viewpoint or ZombieBuddy, which usually run only on players' PCs. Check its Workshop page before leaving it out.")) or nil }
    end
    local q
    local function count()
        local n = 0
        for _, c in ipairs(checks) do if not c.on then n = n + 1 end end
        return T("leaveout_count", "%1 of %2 left out", n, #checks)
    end
    q = { title = T("leaveout_title", "Mods this server gets") .. ": " .. name, checks = checks,
        lines = { T("leaveout_hint", "Untick mods the server shouldn't get, like client-side add-ons. PZMM remembers this for %1.", name), count() },
        choices = {
        { "Give it my mods", function()
            local now = {}
            for _, c in ipairs(checks) do if not c.on then now[c.id] = true end end
            self:saveLeaveOut(name, now)
            local ids, items, maps = K.serverLists(order, now, function(id) return wsOf(mods[id]) end, self:maps(), function(id)
                local out = {}
                pcall(function()
                    local a = getMapFoldersForMod(id)
                    for i = 0, (a and a:size() or 0) - 1 do out[#out + 1] = tostring(a:get(i)) end
                end)
                return out
            end)
            local ok = pcall(function()
                st:loadFiles()
                local o = st:getServerOptions()
                -- same style as the file: B42 writes "\id", B41 "id" (empty: by game version)
                local modsOpt = o:getOptionByName("Mods")
                local cur = tostring(modsOpt:getValue() or "")
                local pre = cur:match("[^;%s]") ~= nil and cur:match("^[%s;]*(.)") == "\\"
                if cur:match("[^;%s]") == nil then
                    local major = 42
                    pcall(function() major = getCore():getGameVersion():getMajor() end)
                    pre = major >= 42
                end
                local mids = {}
                for i, id in ipairs(ids) do mids[i] = (pre and "\\" or "") .. id end
                modsOpt:setValue(#mids > 0 and table.concat(mids, ";") or (pre and "\\" or ""))
                o:getOptionByName("WorkshopItems"):setValue(table.concat(items, ";"))
                if #maps > 0 then o:getOptionByName("Map"):setValue(table.concat(maps, ";")) end
                st:saveFiles()
            end)
            if self.srvSel and self.srvSel.hosted == st then self.srvSel.opts = nil end
            self:say(ok and (name .. " has your " .. #ids .. " mods now.") or ("Couldn't change " .. name .. "'s settings."))
            -- a kept mod that needs one left out (as the app's check), once the server has the list
            for _, w in ipairs(ok and K.needsLeftOut(order, now, reqs) or {}) do
                self:say(T("check_needs_left_out", "%1 needs %2, which you left out for this server", mods[w[1]] and mods[w[1]].name or w[1], mods[w[2]] and mods[w[2]].name or w[2]), "error")
            end
        end },
    } }
    -- ticking updates the count line
    q.recount = function() q.lines[2] = count() end
    self:ask(q)
end

--- Put the installed mods of Workshop items `wsIds` (and the mods they require) into a hosted server's Mods= and
--- WorkshopItems= (a menu of your hosted servers, then asks first).
function PZMMMods:addToHosted(wsIds, title)
    local want = {}
    for _, w in ipairs(wsIds) do want[tostring(w)] = true end
    local ids, items, seenId, seenItem = {}, {}, {}, {}
    local function add(d, depth)
        if seenId[d.modId] or depth > 8 then return end
        seenId[d.modId] = true
        for _, q in ipairs(rules(d).requires) do
            local rd = self.model.mods[q]
            if rd then add(rd, depth + 1) end
        end
        ids[#ids + 1] = d.modId
        local ws = wsOf(d)
        ws = tostring(ws or "")
        if ws ~= "" and ws ~= "0" and not seenItem[ws] then seenItem[ws] = true; items[#items + 1] = ws end
    end
    for _, d in pairs(self.model.mods) do
        local ws = wsOf(d)
        if ws and want[tostring(ws)] then add(d, 0) end
    end
    if #ids == 0 then return self:say("None of its mods are installed here: subscribe first.") end
    local servers = self:hostedList()
    if #servers == 0 then return self:say("You host no servers here yet: host one first.") end
    local entries = {}
    for _, st in ipairs(servers) do
        local name = ""
        pcall(function() name = st:getName() end)
        entries[#entries + 1] = { name, function()
            self:ask({ title = "Add " .. title .. " to " .. name .. "?", lines = { #ids .. (#ids == 1 and " mod" or " mods") .. " go into its Mods= and WorkshopItems=, the ones it lacks. Restart it for it to take effect." }, choices = {
                { "Add to server", function()
                    local added = 0
                    local ok = pcall(function()
                        st:loadFiles()
                        local o = st:getServerOptions()
                        local function merge(opt, list, isMods)
                            local cur, have, n, pre = {}, {}, 0, nil
                            for v in tostring(opt:getValue() or ""):gmatch("[^;]+") do
                                v = v:gsub("^%s+", ""):gsub("%s+$", "")
                                if v ~= "" then
                                    if pre == nil then pre = isMods and v:sub(1, 1) == "\\" end
                                    have[v:gsub("^.*\\", "")] = true
                                    cur[#cur + 1] = v
                                end
                            end
                            if pre == nil then
                                local major = 42
                                pcall(function() major = getCore():getGameVersion():getMajor() end)
                                pre = isMods and major >= 42
                            end
                            for _, v in ipairs(list) do
                                if not have[v] then have[v] = true; cur[#cur + 1] = (pre and "\\" or "") .. v; n = n + 1 end
                            end
                            opt:setValue(table.concat(cur, ";"))
                            return n
                        end
                        added = merge(o:getOptionByName("Mods"), ids, true)
                        merge(o:getOptionByName("WorkshopItems"), items)
                        st:saveFiles()
                    end)
                    if self.srvSel and self.srvSel.hosted == st then self.srvSel.opts = nil end
                    self:say(not ok and ("Couldn't change " .. name .. "'s settings.") or added == 0 and (name .. " already has them.") or (added .. (added == 1 and " mod" or " mods") .. " added to " .. name .. "."))
                end },
            } })
        end }
    end
    self:menu(entries)
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
            if jar and d.modId ~= "ZombieBuddy" then
                -- the jar mod.info names: there (in the build's folder or common/), or missing
                local has = false
                pcall(function()
                    local j = getModFileReader(d.modId, jar, false)
                    if j then has = true; j:close() end
                end)
                self.java[#self.java + 1] = { d = d, jar = jar, zb = needsZB, missing = not has }
            end
        end)
    end
    table.sort(self.java, function(a, b) return (a.d.name or "") < (b.d.name or "") end)
    return self.java, #self.javaTodo == 0
end

function PZMMMods:drawJava(x, y, w, mx, my)
    y = self:drawNotice(x, y, w, T("notice_java", "setting up, repairing or removing ZombieBuddy, scanning inside each jar, checking each jar's signature, approving Java mods, setting up Project Viewpoint, other Java loaders and copying a jar's SHA-256."), mx, my)
    -- ZombieBuddy: running, or why not
    local zb = self.model.mods["ZombieBuddy"]
    local version
    if ZombieBuddy and ZombieBuddy.getVersion then pcall(function() version = ZombieBuddy.getVersion() end) end
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
    local lines = wrap(status, w - 32)
    local rowH = 30
    local h = 14 + H_SMALL + 10 + 3 * rowH + #lines * (H_SMALL + 2) + 14
    card(self, x, y, w, h)
    bold(self, "ZombieBuddy", x + 16, y + 14, C.text)
    -- what makes it work, one after the other: installed, turned on, running
    local steps = {
        { T("java_installed", "Installed"), zb ~= nil },
        { T("java_on", "Turned on"), zb ~= nil and zb.isActive == true },
        { version and (T("java_running", "Running") .. ", v" .. tostring(version)) or T("java_running", "Running"), version ~= nil },
    }
    local sy = y + 14 + H_SMALL + 10
    for i, st in ipairs(steps) do
        local ry = sy + (i - 1) * rowH
        local tone = st[2] and C.ok or C.muted
        round(self, x + 16, ry + 4, 22, 22, 10, tone, st[2] and 0.22 or 0.1)
        outline(self, x + 16, ry + 4, 22, 22, 10, tone, 0.7)
        if st[2] and TB.check then self:drawTextureScaled(TB.check, x + 20, ry + 8, 14, 14, 1, tone.r, tone.g, tone.b) end
        text(self, st[1], x + 48, ry + 4 + (22 - H_SMALL) / 2, st[2] and C.text or C.muted)
    end
    local sty = sy + 3 * rowH + 2
    for _, l in ipairs(lines) do
        text(self, l, x + 16, sty, c)
        sty = sty + H_SMALL + 2
    end
    y = y + h + 12
    -- their Workshop pages (the Java loader, and Project Viewpoint that rides on it)
    do
        local bx = x
        for _, b in ipairs({
            { "ZombieBuddy Workshop page", "3619862853", "ZombieBuddy on the Workshop." },
            { "Viewpoint Workshop page", "3809306528", "Project Viewpoint on the Workshop." },
            { T("zb_about", "About ZombieBuddy"), "https://github.com/zed-0xff/ZombieBuddy", "ZombieBuddy's page on GitHub." },
            { T("zb_installer_btn", "Windows installer"), "https://github.com/zed-0xff/ZombieBuddy/releases/tag/windows_installer", "If you use the alternate launcher or Steam launch options, use the official installer instead." },
            { T("zb_viewpoint_discord", "Viewpoint Discord"), "https://discord.gg/g83XvPhEgQ", "Project Viewpoint's community Discord: help, bug reports, news" },
        }) do
            local isUrl = b[2]:find("^https?://") ~= nil
            local bw, bh2, over = flatButton(self, b[1], bx, y, mx, my, nil, isUrl and TB.external or TB.steam)
            if over then tip(self, b[1] .. "\n" .. b[3]) end
            local id = b[2]
            self.feedHits[#self.feedHits + 1] = { kind = isUrl and "url" or "fn", url = id, fn = function() self:openWorkshop(id) end, x = bx, y = y, w = bw, h = bh2 }
            bx = bx + bw + 8
        end
        y = y + BAR + 20
    end
    -- Project Viewpoint (rides on ZombieBuddy), when installed: its version and whether ZombieBuddy is ready for it
    local vp = self.model.mods["Viewpoint"]
    if vp then
        local vver = ""
        pcall(function() vver = tostring(vp.modInfo:getModVersion() or "") end)
        local l1 = "Installed" .. (vver ~= "" and (", version " .. vver) or "") .. (vp.isActive and ", turned on." or ", turned off.")
        local l2, c2 = "ZombieBuddy is running: Viewpoint's Java side can load.", C.ok
        if not version then l2, c2 = "ZombieBuddy isn't running: Viewpoint's Java side won't load until it is.", C.warn end
        local vh = 14 + H_SMALL + 8 + (H_SMALL + 4) * 2 + 10
        card(self, x, y, w, vh)
        bold(self, T("java_viewpoint", "Project Viewpoint"), x + 16, y + 14, C.text)
        text(self, fit(l1, w - 32), x + 16, y + 14 + H_SMALL + 8, C.muted)
        text(self, fit(l2, w - 32), x + 16, y + 14 + H_SMALL + 8 + H_SMALL + 4, c2)
        y = y + vh + 20
    end
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
        local over = listRow(self, x, ry, w, ROW, i == #list, mx, my)
        local d = j.d
        local mid = ry + (ROW - H_SMALL) / 2
        local right = pill(self, d.isActive and "Enabled" or "Off", x + w - 16, ry + (ROW - H_SMALL - 4) / 2, d.isActive and C.ok or C.muted, d.isActive)
        local tx = x + 16
        local name = fit(d.name or d.modId, right - 16 - tx - 200)
        bold(self, name, tx, mid, d.isActive and C.text or C.muted)
        tx = tx + width(name) + 12
        text(self, fit(j.jar, right - 16 - tx - 150), tx, mid, copy(C.muted, 0.75))
        local jw = width(fit(j.jar, right - 16 - tx - 150))
        if j.missing then
            chip(self, "Jar missing", tx + jw + 12, ry + (ROW - H_SMALL - 2) / 2, C.danger)
        elseif j.zb and not version then
            chip(self, "Needs ZombieBuddy", tx + jw + 12, ry + (ROW - H_SMALL - 2) / 2, C.danger)
        end
        if over then tip(self, (d.name or d.modId) .. "\n" .. j.jar .. (j.missing and "\nIts mod.info names this jar, but it isn't in the mod: reinstall the mod." or "") .. "\nClick to show it in My mods.") end
        self.feedHits[#self.feedHits + 1] = { kind = "fn", fn = function()
            self.page, self.selected, self.tab = "mods", d, "All"
            self.search:setText("")
            self:updateView()
        end, x = x, y = ry, w = w, h = ROW }
    end
    return y + lh
end

-- Settings --------------------------------------------------------------------------------------------

K.SETTING_ROWS = {
    { "logo", "Hide the Project Zomboid logo on the main menu" },
    { "watermark", "Hide ZombieBuddy's watermark on the main menu", "Its \"loaded\" line and Java mods list over the top of the screen." },
    { "news", "PZMM patch notes box on the main menu", "The box at the top right, with a red ! for a new patch." },
    { "tips", "Tooltips", "What a button, tab or chip does, when you hover it." },
    { "large", "Larger text", "One font size up on this screen." },
    { "ids", T("set_ids", "Show mod IDs beside names"), "In My mods' rows." },
    { "favfirst", T("set_favfirst", "Favourites at the top of sorted and filtered lists") },
    { "compact", T("set_compact", "Compact rows"), "Shorter rows in My mods." },
}

--- Whether the app pitch under Settings > About was dismissed (Zomboid/Lua/pzmm/pitch.txt).
local function pitchDismissed()
    -- drawn every frame: read at most every 2 s
    local t = getTimestampMs()
    if PZMMMods.pitchAt and t - PZMMMods.pitchAt < 2000 then return PZMMMods.pitchVal end
    local ok, v = pcall(function()
        local r = getFileReader("pzmm/pitch.txt", false)
        if not r then return false end
        local l = r:readLine()
        r:close()
        return l == "0"
    end)
    PZMMMods.pitchAt, PZMMMods.pitchVal = t, ok and v or false
    return PZMMMods.pitchVal
end

function PZMMMods:drawSettings(x, y, w, mx, my)
    y = self:drawNotice(x, y, w, T("notice_settings", "language, the game build override, notifications and the tray, folders, downloads and updates, accounts and keys, removal and backup options, theme and accent, launch options, Performance (memory, JVM arguments, PZOpt, the largest mods), the Workshop links open in Steam or the browser setting, the server-country setting, the app's update check, autosorting new mods, autostart, which page the Workshop opens on, results per page, re-downloading the mods PZMM installed, keeping favourites on and hidden mods off in every profile, and a profile's own game folder (its saves, options and logs kept apart)."), mx, my)
    local rh = H_SMALL * 2 + 22
    -- its heading and description are in the card (22 above, 26 at the sides)
    local head = 22 + H_MEDIUM + 4 + H_SMALL + 14
    local h = head + #K.SETTING_ROWS * rh
    card(self, x, y, w, h)
    bold(self, "In-game mod manager", x + 26, y + 22, C.text, MEDIUM)
    text(self, T("set_lede", "This screen's options. PZMM's Settings write the same file."), x + 26, y + 22 + H_MEDIUM + 4, C.muted)
    rect(self, x, y + head - 1, w, 1, C.line, 1)
    for i, s in ipairs(K.SETTING_ROWS) do
        local ry = y + head + (i - 1) * rh
        -- label on the left, the checkbox on the right, a line between rows; no hover tint
        if i < #K.SETTING_ROWS then rect(self, x, ry + rh - 1, w, 1, C.line, 1) end
        checkbox(self, x + w - 26 - BOX, ry + (rh - BOX) / 2, SETTINGS[s[1]])
        local tx = x + 26
        if s[3] then
            text(self, s[2], tx, ry + 10, C.text)
            text(self, s[3], tx, ry + 12 + H_SMALL, C.muted)
        else
            text(self, s[2], tx, ry + (rh - H_SMALL) / 2, C.text)
        end
        local key = s[1]
        self.feedHits[#self.feedHits + 1] = { kind = "fn", fn = function()
            SETTINGS[key] = not SETTINGS[key]
            saveSettings(key)
            -- Larger text: the screen is built again at the new size, on this page
            if key == "favfirst" then self:updateView() end
            if key == "large" or key == "compact" then
                local page = self.page
                local keep = { defs = {}, favs = {}, orig = self.modsOrig }
                for id, d in pairs(self.model.mods) do keep.defs[id] = { d.defaultActive, d.defaultFav } end
                for id, v in pairs(self.model.favs or {}) do keep.favs[id] = v end
                pcall(function()
                    keep.cur = ActiveMods.getById("pzmmCarry")
                    keep.cur:copyFrom(self.model:getActiveMods())
                end)
                keep.bridge = { pending = self.pending, sent = self.sent, sentAt = self.sentAt, kinds = self.kinds, afterAck = self.afterAck,
                    overlay = self.overlay, defRev = self.defRev, defChanged = self.defChanged }
                PZMMMods.carry = keep
                self:close()
                PZMMMods.open()
                PZMMMods.instance.page = page
            end
        end, x = x, y = ry, w = w, h = rh }
    end
    y = y + h + 20
    -- Make a mod, as far as the game goes: your own Workshop items
    local label = T("make_items", "Your Workshop items")
    local bw = width(label) + 46
    local tw, over = toolButton(self, TB.layers, label, x, y, mx, my)
    if over then tip(self, "Your Workshop items\nMods you made: open their files, and upload an update from the game's Workshop screen.") end
    self.feedHits[#self.feedHits + 1] = { kind = "fn", fn = function() self.page, self.feedScroll = "make", 0 end, x = x, y = y, w = tw, h = BAR }
    y = y + BAR + 20
    bold(self, "About", x, y, C.text)
    y = y + H_SMALL + 10
    -- the PZMM app, once: what it adds, Get the app, and Don't show this again
    if not self.app and not pitchDismissed() then
        local lines = wrap("The PZMM Mod Manager app adds the rest: news and change notes, Workshop pictures, installs without Steam, backups and restore, profiles and server tools. With both, everything here syncs with it.", w - 32)
        local ph = 16 + #lines * (H_SMALL + 2) + 10 + BAR + 16
        card(self, x, y, w, ph)
        local ty = y + 16
        for _, l in ipairs(lines) do
            text(self, l, x + 16, ty, C.text)
            ty = ty + H_SMALL + 2
        end
        ty = ty + 10
        local gx = x + 16
        local gw2, _, o1 = flatButton(self, "Get the app", gx, ty, mx, my, "primary")
        if o1 then tip(self, "Get the app\n" .. K.RELEASES_URL) end
        self.feedHits[#self.feedHits + 1] = { kind = "url", url = K.RELEASES_URL, x = gx, y = ty, w = gw2, h = BAR }
        local dx = gx + gw2 + 8
        local dw, _, o2 = flatButton(self, "Don't show this again", dx, ty, mx, my, "quiet")
        if o2 then tip(self, "Don't show this again") end
        self.feedHits[#self.feedHits + 1] = { kind = "fn", fn = function()
            pcall(function()
                local wr = getFileWriter("pzmm/pitch.txt", true, false)
                wr:write("0")
                wr:close()
            end)
            PZMMMods.pitchAt = nil
        end, x = dx, y = ty, w = dw, h = BAR }
        y = y + ph + 12
    end
    local game, app = "", self.app and self.app.version or "not installed"
    for _, n in ipairs(K.NOTES or {}) do if n.kind == "game" then game = n.version end end
    text(self, "In-game mod manager " .. (game ~= "" and game or "?") .. "    PZMM app " .. app, x, y, C.muted)
    y = y + H_SMALL + 14
    -- the Logs folder, and what to paste into a bug report
    local bx = x
    for _, b in ipairs({
        { T("about_logs", "Open the Logs folder"), "Open the Logs folder\nZomboid/Logs: the game's console and error logs.", function() K.openLogs() end, TB.folder },
        { T("about_diag", "Copy diagnostics"), "Copy diagnostics\nThe versions, mod counts, folder and PZMM app state as text, for a bug report.", function()
            pcall(function() Clipboard.setClipboard(K.diagnostics(self, game)) end)
            self:say("Diagnostics copied.")
        end, TB.copy },
    }) do
        local bw, bh2, o = flatButton(self, b[1], bx, y, mx, my, nil, b[4])
        if o then tip(self, b[2]) end
        self.feedHits[#self.feedHits + 1] = { kind = "fn", fn = b[3], x = bx, y = y, w = bw, h = bh2 }
        bx = bx + bw + 8
    end
    y = y + BAR + 20
    -- the shortcuts this screen answers to
    bold(self, T("about_keys", "Keyboard shortcuts"), x, y, C.text)
    y = y + H_SMALL + 10
    local keysH = #K.SHORTCUTS * (H_SMALL + 6) + 20
    card(self, x, y, w, keysH)
    local kw = 0
    for _, k in ipairs(K.SHORTCUTS) do kw = math.max(kw, width(k[1])) end
    for i, k in ipairs(K.SHORTCUTS) do
        local ky = y + 10 + (i - 1) * (H_SMALL + 6)
        bold(self, k[1], x + 20, ky, C.text)
        text(self, fit(k[2], w - 40 - kw - 24), x + 20 + kw + 24, ky, C.muted)
    end
    return y + keysH
end

-- Browse Workshop and Make a mod --------------------------------------------------------


--- An item or collection the Workshop tab looked up: Steam's details, by ID (asked once).
function PZMMMods:lookUp(id)
    self.looked = self.looked or {}
    local was = self.looked[id]
    -- asked once; one Steam couldn't answer is asked again (Retry)
    self.lookOrder = self.lookOrder or {}
    if was and not was.failed then
        -- already fetched (a collection's child, a requirement): bring it to the front of the list
        for i, v in ipairs(self.lookOrder) do
            if v == id then table.remove(self.lookOrder, i); break end
        end
        table.insert(self.lookOrder, 1, id)
        return
    end
    self.looked[id] = { id = id, asking = true, at = getTimestampMs() }
    if not was then table.insert(self.lookOrder, 1, id) end
    self:askSteam({ id })
end

--- Ask Steam for these IDs' details; the ones it can't answer end up marked failed.
function PZMMMods:askSteam(ids)
    local ok = pcall(function()
        local a = ArrayList.new()
        for _, id in ipairs(ids) do a:add(id) end
        querySteamWorkshopItemDetails(a, function(_, status, info) self:onLookedUp(status, info, ids) end, self)
    end)
    if not ok then self:lookFailed(ids) end
end
function PZMMMods:lookFailed(ids)
    for _, id in ipairs(ids) do
        local e = self.looked and self.looked[id]
        if e and e.asking then e.asking, e.failed = false, true end
    end
end
function PZMMMods:onLookedUp(status, info, ids)
    if status ~= "Completed" or not info then
        if not info and ids and tostring(status):lower():find("fail") then self:lookFailed(ids) end
        return
    end
    for i = 0, info:size() - 1 do
        pcall(function()
            local det = info:get(i)
            local id = det:getIDString()
            local e = self.looked[id] or { id = id }
            e.asking, e.title, e.size = false, det:getTitle(), 0
            -- Steam answers an ID it doesn't know with an empty item
            if not e.title or e.title == "" then e.failed = true end
            pcall(function() e.size = tonumber(tostring(det:getFileSize())) or 0 end)
            pcall(function() e.updated = tonumber(tostring(det:getTimeUpdated())) or 0 end)
            pcall(function() e.created = tonumber(tostring(det:getTimeCreated())) or 0 end)
            pcall(function() e.state = tostring(det:getState()) end)
            e.children = {}
            pcall(function()
                for j = 0, det:getNumChildren() - 1 do e.children[#e.children + 1] = tostring(det:getChildID(j)) end
            end)
            self.looked[id] = e
            -- a collection's items: asked about too, for their titles
            if #e.children > 0 then
                local ask = {}
                for _, c in ipairs(e.children) do
                    if not self.looked[c] then
                        self.looked[c] = { id = c, asking = true, child = true, at = getTimestampMs() }
                        ask[#ask + 1] = c
                    end
                end
                if #ask > 0 then self:askSteam(ask) end
            end
        end)
    end
    -- asked for but not in the answer: Steam doesn't know it
    if ids then self:lookFailed(ids) end
end

--- A Workshop ID from what was typed: the digits of an id=... link, or the digits alone.
local function workshopId(typed)
    typed = tostring(typed or "")
    local id = typed:match("[?&]id=(%d+)") or typed:match("^%s*(%d+)%s*$")
    return id and #id <= 20 and id or nil
end

local function megabytes(bytes)
    if not bytes or bytes <= 0 then return nil end
    local mb = bytes / 1048576
    return mb >= 10 and (math.floor(mb + 0.5) .. " MB") or (math.floor(mb * 10 + 0.5) / 10 .. " MB")
end

--- Browse Workshop, as far as the game lets a mod go: search the Workshop in Steam, and look up any item or
--- collection by its ID or link (its size, when it was updated, whether you have it, a collection's items),
--- with Subscribe (Steam) or Install with PZMM.
function PZMMMods:drawWorkshop(x, y, w, mx, my, top, bh)
    y = self:drawNotice(x, y, w, T("notice_workshop", "browsing the Workshop with filters, tags, sorting and pages, an item's stars, comments and pictures, making a profile from a collection, the mods-by-this-author and mods-that-need drill-downs, the item's change-notes tab, Collections with this item (that needs Workshop queries), its description, subscriber, visitor and favourite counts and tags, the Workshop's Home view (popular this week, most subscribed, newest, recently updated, most required), and removing an installed item."), mx, my)
    local bx = x
    local function button(label, kind, tipText, fn, icon)
        local bw, _, over = flatButton(self, label, bx, y, mx, my, kind, icon)
        if over then tip(self, tipText) end
        self.feedHits[#self.feedHits + 1] = { kind = "fn", fn = fn, x = bx, y = y, w = bw, h = BAR }
        bx = bx + bw + 8
    end
    local function search(q)
        local url = "https://steamcommunity.com/workshop/browse/?appid=108600&searchtext=" .. urlEncode(q)
        local overlay = false
        pcall(function() overlay = isSteamOverlayEnabled() end)
        if overlay then pcall(function() activateSteamOverlayToWebPage(url) end) else openLink(url) end
    end
    button("Search the Workshop", "primary", "Search the Workshop\nOpens Steam's Workshop search for Project Zomboid with what you type.", function()
        self:prompt("Search the Workshop", "", "Search", function(q)
            search((tostring(q or ""):gsub("^%s+", ""):gsub("%s+$", "")))
        end)
    end)
    button("Look up an ID or link", nil, "Look up an ID or link\nA Workshop item or collection: paste its link or type its ID.", function()
        self:prompt("Look up a Workshop item or collection", "", "Look up", function(typed)
            local id = workshopId(typed)
            if not id then return self:say("That isn't a Workshop ID or link: it's the number after id= in the page's address.") end
            self:lookUp(id)
        end)
    end)
    button("Open the Workshop", "quiet", "Open the Workshop\nProject Zomboid's Workshop in Steam.", function() openLink("https://steamcommunity.com/app/108600/workshop/") end)
    button(T("ws_hide_installed", "Hide installed"), "quiet", "Hide installed\nLeaves out the looked-up items you already have.", function() self.wsHide = not self.wsHide end, self.wsHide and TB.check or nil)
    if self.model.mods["Viewpoint"] then
        button(T("ws_viewpoint_addons", "Viewpoint add-ons"), "quiet", "Viewpoint add-ons\nSearches the Workshop for Project Viewpoint's add-ons.", function() search("Project Viewpoint") end, TB.layers)
        button(T("ws_viewpoint_discord", "Viewpoint Discord"), "quiet", "Viewpoint Discord\nThe Project Viewpoint community.", function() openLink("https://discord.gg/g83XvPhEgQ") end, TB.external)
    end
    y = y + BAR + 20
    local list = {}
    local t = getTimestampMs()
    for _, id in ipairs(self.lookOrder or {}) do
        local e = self.looked[id]
        list[#list + 1] = e
        -- no answer in 15 s counts as failed
        if e.asking and e.at and t - e.at > 15000 then e.asking, e.failed = false, true end
    end
    for _, e in ipairs(list) do
        for _, c in ipairs(e.children or {}) do
            local ce = self.looked[c]
            if ce and ce.asking and ce.at and t - ce.at > 15000 then ce.asking, ce.failed = false, true end
        end
    end
    if #list == 0 then
        text(self, "Look up an item or a collection and it shows here, with what you need to get it.", x, y, C.muted)
        return y + H_SMALL
    end
    -- what you have: subscribed items (Steam's word), and installed mods' Workshop IDs
    local have = {}
    pcall(function()
        local ids = getSteamWorkshopItemIDs()
        for i = 0, ids:size() - 1 do have[tostring(ids:get(i))] = true end
    end)
    for _, d in pairs(self.model.mods) do
        local w = wsOf(d)
        if w ~= "" then have[w] = true end
    end
    -- Steam lists a mod's requirements as children too: only an item with no file of its own is a collection
    local function isColl(e) return #(e.children or {}) > 0 and (e.size or 0) == 0 end
    local function itemRow(e, ry, indent)
        local tx = x + 16 + indent
        local mid = ry + (ROW - H_SMALL) / 2
        local title = e.asking and ("Asking Steam about " .. e.id .. "...") or e.failed and ("Steam didn't find " .. e.id) or (e.title ~= "" and e.title or e.id)
        local right = x + w - 16
        local owned = have[e.id]
        if e.failed then
            local label = "Retry"
            local bw = width(label) + 28
            right = right - bw
            local _, _, over = flatButton(self, label, right, ry + (ROW - BAR) / 2, mx, my)
            if over then tip(self, "Retry\nAsk Steam about " .. e.id .. " again.") end
            self.feedHits[#self.feedHits + 1] = { kind = "fn", fn = function() self:lookUp(e.id) end, x = right, y = ry + (ROW - BAR) / 2, w = bw, h = BAR }
            right = right - 12
        elseif not e.asking and not isColl(e) then
            local label = owned and "Subscribed" or (self.app and "Install with PZMM" or "Subscribe")
            local kind = owned and "quiet" or "primary"
            local icon = (not owned) and TB.download or nil
            local bw = width(label) + 28 + (icon and 20 or 0)
            right = right - bw
            local _, _, over = flatButton(self, label, right, ry + (ROW - BAR) / 2, mx, my, kind, icon)
            if over then tip(self, owned and "Subscribed\nOpens it in Steam." or self.app and (label .. "\nOpens PZMM, which installs it once you say yes.") or "Subscribe\nOpens it in Steam: subscribe there and it downloads.") end
            local act = (not owned and self.app) and function() self:request("install\t" .. e.id, title .. " asked for.") end or function() self:openWorkshop(e.id) end
            self.feedHits[#self.feedHits + 1] = { kind = "fn", fn = act, x = right, y = ry + (ROW - BAR) / 2, w = bw, h = BAR }
            right = right - 12
            if owned then
                local hl = T("ws_add_hosted", "Add to a hosted server")
                local hw = width(hl) + 28
                right = right - hw
                local _, _, hover = flatButton(self, hl, right, ry + (ROW - BAR) / 2, mx, my, "quiet")
                if hover then tip(self, "Add to a hosted server\nIts installed mods, and what they need, join a server's Mods= and WorkshopItems=.") end
                self.feedHits[#self.feedHits + 1] = { kind = "fn", fn = function() self:addToHosted({ e.id }, title) end, x = right, y = ry + (ROW - BAR) / 2, w = hw, h = BAR }
                right = right - 12
            end
        end
        local facts = {}
        if megabytes(e.size) then facts[#facts + 1] = megabytes(e.size) end
        if (e.updated or 0) > 0 then
            local days = math.floor((now() - e.updated) / 86400)
            facts[#facts + 1] = "updated " .. (days <= 0 and "today" or days == 1 and "yesterday" or days < 60 and (days .. " days ago") or (math.floor(days / 30) .. " months ago"))
        end
        if (e.created or 0) > 0 then
            local okc, cs = pcall(os.date, "%d %b %Y", K.LT(e.created))
            if okc and cs then facts[#facts + 1] = "posted " .. cs end
        end
        local meta = table.concat(facts, "    ")
        local mw = meta ~= "" and width(meta) + 16 or 0
        if meta ~= "" then text(self, meta, right - mw + 16 - 16, mid, C.muted) end
        bold(self, fit(title, right - mw - tx - 16), tx, mid, (e.asking or e.failed) and C.muted or C.text)
    end
    local shownList = {}
    for _, e in ipairs(list) do
        if not (self.wsHide and have[e.id] and not isColl(e)) then shownList[#shownList + 1] = e end
    end
    if #shownList == 0 then
        text(self, "Everything you looked up is installed: Hide installed is on.", x, y, C.muted)
        return y + H_SMALL
    end
    for _, e in ipairs(shownList) do
        local kids = e.children or {}
        local rows = 1 + #kids
        local h = rows * ROW + (#kids > 0 and BAR + 16 or 0)
        card(self, x, y, w, h)
        itemRow(e, y, 0)
        local ry = y + ROW
        if #kids > 0 then
            -- a collection: all of it at once (PZMM), or each item in Steam
            local missing = {}
            for _, c in ipairs(kids) do if not have[c] then missing[#missing + 1] = c end end
            text(self, (isColl(e) and ("Collection: " .. #kids .. " items, " .. #missing .. " you don't have.") or ("Needs " .. #kids .. " (" .. #missing .. " you don't have)")), x + 16, ry + (BAR - H_SMALL) / 2, C.muted)
            if #missing > 0 and self.app then
                local label = "Install the " .. #missing .. " with PZMM"
                local bw = width(label) + 48
                local bxx = x + w - 16 - bw
                local _, _, over = flatButton(self, label, bxx, ry, mx, my, "primary", TB.download)
                if over then tip(self, label .. "\nOpens PZMM, which installs them once you say yes.") end
                self.feedHits[#self.feedHits + 1] = { kind = "fn", fn = function() self:request("install\t" .. table.concat(missing, ","), #missing .. " items asked for.") end, x = bxx, y = ry, w = bw, h = BAR }
            end
            if #kids > #missing then
                local label = T("ws_add_hosted", "Add to a hosted server")
                local bw = width(label) + 28
                local bxx = x + w - 16 - bw - ((#missing > 0 and self.app) and (width("Install the " .. #missing .. " with PZMM") + 48 + 8) or 0)
                local _, _, over = flatButton(self, label, bxx, ry, mx, my, "quiet")
                if over then tip(self, label .. "\nThe installed mods of these items, and what they need, join a server's Mods= and WorkshopItems=.") end
                self.feedHits[#self.feedHits + 1] = { kind = "fn", fn = function() self:addToHosted(kids, e.title ~= "" and e.title or e.id) end, x = bxx, y = ry, w = bw, h = BAR }
            end
            ry = ry + BAR + 16
            for _, c in ipairs(kids) do
                itemRow(self.looked[c] or { id = c, asking = true }, ry, 24)
                ry = ry + ROW
            end
        end
        y = y + h + 12
    end
    return y
end

-- the game's upload check (SteamWorkshopItem:validateContents) answers with a code
K.LINT_CODES = {
    MissingContents = "The item's Contents folder is missing.",
    PreviewNotFound = "preview.png is missing next to the Contents folder.",
    PreviewFileSize = "preview.png is too big: the Workshop takes up to 1 MB.",
    PreviewDimensions = "preview.png's size isn't accepted by the Workshop.",
    PreviewFormat = "preview.png isn't a valid PNG.",
    MissingModDotInfo = "A mod folder has no mod.info.",
    InvalidModDotInfo = "A mod.info has no id=, or two mods share an id.",
    FolderNotAllowedInContents = "Contents may only hold buildings/, creative/ and mods/.",
    FileTypeNotAllowed = "Contents holds a file type the Workshop refuses (exe, dll, bat, app, dylib, sh, so, zip).",
}

--- Check one of your Workshop items: the game's own upload check, then each mod.info in it (id, name, poster,
--- requirements, versionMin / versionMax). Stored in self.lint[folder] as { level, text } lines.
function PZMMMods:lintItem(it)
    local out = {}
    local function add(level, text0) out[#out + 1] = { level = level, text = text0 } end
    local folder = ""
    pcall(function() folder = it:getContentFolder() or "" end)
    local code
    pcall(function() code = it:validateContents() end)
    if code and tostring(code) ~= "" then add("error", K.LINT_CODES[tostring(code)] or ("The game's upload check says " .. tostring(code) .. ".")) end
    local function norm(p) return (tostring(p or ""):gsub("\\", "/"):gsub("/+$", ""):lower()) end
    local root = norm(folder) .. "/"
    local running
    pcall(function() running = getCore():getGameVersion() end)
    local checked = 0
    for _, d in pairs(self.model.mods) do
        local dir = ""
        pcall(function() dir = norm(d.modInfo:getDir()) end)
        if folder ~= "" and dir:sub(1, #root) == root then
            checked = checked + 1
            local who = d.name .. ": "
            local info = d.modInfo
            if not d.modId:match("^[%w_%.%-]+$") then add("warn", who .. "the mod ID " .. d.modId .. " has spaces or special characters: keep to letters, digits, _ - and .") end
            if d.name == "" or d.name == d.modId then add("warn", who .. "no name= in mod.info: the mod list shows its ID.") end
            local posters = 0
            pcall(function() posters = info:getPosterCount() or 0 end)
            if posters == 0 then
                add("warn", who .. "no poster= in mod.info: the mod shows no picture.")
            else
                local tex
                pcall(function() tex = getTexture(info:getPoster(0)) end)
                if not tex then add("warn", who .. "its poster file wasn't found where mod.info says.") end
            end
            local desc = ""
            pcall(function() desc = info:getDescription() or "" end)
            if desc:gsub("%s", "") == "" then add("info", who .. "no description= in mod.info.") end
            local seen = {}
            for _, q in ipairs(rules(d).requires) do
                if q == d.modId then
                    add("error", who .. "it requires itself.")
                elseif seen[q] then
                    add("warn", who .. "require= lists " .. q .. " twice.")
                elseif not self.model.mods[q] then
                    add("warn", who .. "it requires " .. q .. ", which isn't installed here: players need that mod's Workshop item too.")
                end
                seen[q] = true
            end
            pcall(function()
                local lo, hi = info:getVersionMin(), info:getVersionMax()
                if lo and hi and lo:isGreaterThan(hi) then add("error", who .. "versionMin is above versionMax: no build can load it.") end
                if lo and lo:getMajor() < 41 then add("warn", who .. "versionMin " .. lo:toString() .. " is older than Build 41.") end
                if running and lo and lo:isGreaterThan(running) then add("warn", who .. "versionMin " .. lo:toString() .. " is above this build: it won't load here.") end
                if running and hi and running:isGreaterThan(hi) then add("warn", who .. "versionMax " .. hi:toString() .. " is below this build: it won't load here.") end
            end)
        end
    end
    if checked == 0 then add("info", "The game doesn't list a mod from this folder, so mod.info couldn't be read here: put it in Contents/mods/<name>/, then Rescan.") end
    self.lint = self.lint or {}
    self.lint[folder] = { list = out, checked = checked }
    self:say(#out == 0 and "No problems found in the item." or (#out .. (#out == 1 and " finding" or " findings") .. " for the item."))
end

function PZMMMods:drawMake(x, y, w, mx, my, top, bh)
    -- your own Workshop items (Zomboid/Workshop): open their files to edit, upload an update from the game's screen
    local staged = {}
    pcall(function()
        local a = getSteamWorkshopStagedItems()
        for i = 0, a:size() - 1 do staged[#staged + 1] = a:get(i) end
    end)
    if #staged > 0 then
        y = self:drawNotice(x, y, w, T("notice_make", "checking any mod folder and building its upload folder."), mx, my)
        bold(self, T("make_items", "Your Workshop items"), x, y, C.text)
        y = y + H_SMALL + 10
        local rh = H_SMALL * 2 + 28
        -- an item's Check findings open under its row
        local hs, total = {}, 0
        for i, it in ipairs(staged) do
            local folder = ""
            pcall(function() folder = it:getContentFolder() or "" end)
            local lint = self.lint and self.lint[folder]
            local extra = 0
            if lint then
                local lines = 0
                for _, f in ipairs(lint.list) do lines = lines + #wrap(f.text, w - 64) end
                extra = math.max(lines, 1) * (H_SMALL + 2) + 20
            end
            hs[i] = rh + extra
            total = total + hs[i]
        end
        card(self, x, y, w, total)
        local off = 0
        for i, it in ipairs(staged) do
            local ry = y + off
            off = off + hs[i]
            listRow(self, x, ry, w, hs[i], i == #staged, mx, my)
            local title, id, folder, vis = "", "", "", ""
            pcall(function()
                title, folder = it:getTitle() or "", it:getContentFolder() or ""
                id = it:getID() or ""
                vis = it:getVisibility() or ""
            end)
            local bx = x + w - 16
            local acts = {
                (id ~= "" and id ~= "0") and { "Upload update", "primary", "Upload update\nThe game's Workshop screen: pick it there, write a change note and upload.", function() self:gameScreen("WORKSHOP") end }
                    or { "Upload", "primary", "Upload\nThe game's Workshop screen: pick it there and upload it for the first time.", function() self:gameScreen("WORKSHOP") end },
                { "Check", "quiet", "Check\nThe game's upload check, then each mod.info in it: id, name, poster, requirements and game versions.", function() self:lintItem(it) end },
                { "Files", nil, "Files\nOpen its folder to edit it.", function() pcall(function() showFolderInDesktop(folder) end) end },
            }
            if id ~= "" and id ~= "0" then table.insert(acts, 3, { "Workshop page", "quiet", "Workshop page", function() self:openWorkshop(id) end }) end
            for _, a in ipairs(acts) do
                local aw = width(a[1]) + 28
                bx = bx - aw
                local _, _, over = flatButton(self, a[1], bx, ry + (rh - BAR) / 2, mx, my, a[2])
                if over then tip(self, a[3]) end
                self.feedHits[#self.feedHits + 1] = { kind = "fn", fn = a[4], x = bx, y = ry + (rh - BAR) / 2, w = aw, h = BAR }
                bx = bx - 8
            end
            bold(self, fit(title ~= "" and title or folder, bx - 24 - x - 16), x + 16, ry + 12, C.text)
            text(self, fit((id ~= "" and id ~= "0") and ("Workshop " .. id .. (vis ~= "" and ("    " .. vis) or "")) or "Not uploaded yet", bx - 24 - x - 16), x + 16, ry + 16 + H_SMALL, C.muted)
            local lint = self.lint and self.lint[folder]
            if lint then
                local ly = ry + rh - 4
                if #lint.list == 0 then
                    if TB.check then self:drawTextureScaled(TB.check, x + 16, ly + math.floor((H_SMALL - 14) / 2), 14, 14, 1, C.muted.r, C.muted.g, C.muted.b) end
                    text(self, "No problems found (" .. lint.checked .. (lint.checked == 1 and " mod" or " mods") .. " checked).", x + 36, ly, C.text)
                end
                for _, f in ipairs(lint.list) do
                    local c = f.level == "error" and C.danger or C.muted
                    if TB.alert then self:drawTextureScaled(TB.alert, x + 16, ly + math.floor((H_SMALL - 14) / 2), 14, 14, 1, c.r, c.g, c.b) end
                    for _, l in ipairs(wrap(f.text, w - 64)) do
                        text(self, l, x + 36, ly, C.text)
                        ly = ly + H_SMALL + 2
                    end
                end
            end
        end
        return y + total
    end
    y = self:drawNotice(x, y, w, T("notice_make", "checking any mod folder and building its upload folder."), mx, my)
    if getSteamModeActive() then
        local label = "Game's Workshop screen"
        local bw, bh2, over = flatButton(self, label, x, y, mx, my, nil, TB.steam)
        if over then tip(self, "Game's Workshop screen\nUpload a mod you've made.") end
        self.feedHits[#self.feedHits + 1] = { kind = "fn", fn = function() self:gameScreen("WORKSHOP") end, x = x, y = y, w = bw, h = bh2 }
        y = y + bh2
    end
    return y
end

--- The frame every other tab draws in: What's new's (heading, scrolling, scroll bar).
function PZMMMods:drawPage()
    local bx, by, bw, bh = self:feedBox()
    -- each page's column: Settings and Saves are narrower than the rest
    local cw = math.min(bw, ({ settings = 860, saves = 1180 })[self.page] or 1240)
    local x0 = math.floor(bx + (bw - cw) / 2)
    local mx, my = self:getMouseX(), self:getMouseY()
    self.feedHits = {}
    local pg = PAGE[self.page]
    local top = self.page == "home" and TOP or by
    if my < top or my >= by + bh then my = -1e9 end
    local y
    if self.page == "home" then
        -- Home's hero meets the tabs: its frame is the whole width from under them
        bx, by, bw, bh = 0, TOP, self.width, self.height - BOTTOM - TOP
        self:setStencilRect(bx, by, bw, bh)
        y = math.floor(by - self.feedScroll)
    else
        self:setStencilRect(bx, by, bw, bh)
        y = math.floor(by + 8 - self.feedScroll)
        -- (Saves has its heading in its card)
        if self.page ~= "saves" then
            bold(self, pg[2], x0, y, C.text, MEDIUM)
            y = y + H_MEDIUM + 12
        end
    end
    y = self[pg[4]](self, x0, y, cw, mx, my, by, bh)
    self:clearStencilRect()
    self.feedHeight = y + self.feedScroll - by + 24
    self.feedScroll = math.max(0, math.min(self.feedScroll, self.feedHeight - bh))
    scrollbar(self, "feedScroll", bx, by, bw, bh, self.feedHeight)
end

-- the menu built before this file loaded holds the old function: point its Mods item here too
local function hook()
    local main = MainScreen.instance
    local m = main and main.modsOption
    if m then m.onMouseDown = MainScreen.onMenuItemMouseDownMainMenu end
    if main and #K.NOTES > 0 and not (PZMMNews.instance and PZMMNews.instance.parent == main) then
        PZMMNews.instance = PZMMNews:new()
        PZMMNews.instance:initialise()
        main:addChild(PZMMNews.instance)
    end
end
if Events.OnMainMenuEnter then Events.OnMainMenuEnter.Add(hook) end
hook()
