# PZMM 1.2.0: every feature

Everything the app and the `pzmm` command line do today, grouped the way the app is. Button names are as they appear in the English UI. What changed between versions is in [release-notes](release-notes/).

## Platforms

- Windows and Build 42 are what I test on most. That's the safest path.
- Linux works (AppImage for any distribution, or a `.deb`), with the native game or the Windows game under Proton, as on a Steam Deck. It gets less testing, so expect rough edges.
- Build 41 works for mods, load order and joining servers. Build 41 dedicated servers (the `legacy41` branch) work too, but get less testing.
- No Mac build.
- PZMM finds the game in any Steam library, and finds GOG copies.

## Home

- PZMM opens on **Home**: the game's art, your profile or the server you picked, and a **Play** button.
- **Select a server, then click Join**: Popular, Favourites, Recent and Your servers in one list. Click it again, or press Esc, to go back to singleplayer.
- Each server shows a lock if it needs a password, and how many of its mods you still need ("3 to download").
- Popular is the last list PZMM got. **List servers** asks Steam again (Steam shows you as playing for a moment).
- At a glance: last server played, mods, problems and updates. With a server picked: players, ping, map and build.
- A status pill that does the job: **Fix** for setup problems, **Get mods** for a server's missing mods, **Manage** for a server you host.
- **Continue {save}** starts the game when your latest save uses this profile's mods.
- A line when enabled mods need ZombieBuddy and it isn't set up.
- **Copy address**, **Copy join link**, and the server's own Discord and website when it lists them.
- **News from The Indie Stone** with major updates and patches tagged, and "May affect mods" on anything out since you last played.
- **Your mods with new updates**, with their Workshop images.
- A **Community** note: the PZ website, forums, Discord, PZwiki, the modding Discord, the subreddit, the Workshop and the PZMM Discord.

## My mods and load order

### Installing
- Paste a Workshop link, collection or ID, a GitHub repo (`owner/repo`, optionally `@tag`) or a `.zip` path, then **Install**. No Steam login.
- **Browse…** installs from a `.zip` file or a mod folder on this computer.
- Required mods come with it. Only what's needed and compatible gets turned on.
- Mods you subscribe to in Steam show up on their own.
- **Install missing mods** finds enabled mods that aren't installed on the Workshop and installs them.
- In a mod's details, a missing requirement has a find button; if no result is clearly it, you pick from the matches.
- Progress and **Cancel** for downloads.

### The library (left list)
- Your own arrangement, kept apart from the load order, like Mod Organizer 2.
- **Add separator**, double-click to rename, drag a separator to move its whole section, Delete removes it and leaves its mods.
- **Group by category** sorts the library into sections from the mods' Workshop tags. It asks before replacing your own sections.
- **Filters**: favourites, hidden, kind (framework, patch, map, UI, translation), your own tags, and "In my language". The filter and sort are remembered.
- Sort by library order, name, size, last changed, problems first, enabled first or date installed.
- Search with `/`. Arrow keys, Enter for details, Space to toggle, Alt+↑/↓ to move.
- Ctrl/Shift+click, or **Select all** (Ctrl+A), then enable, disable, **Move to…** or remove them together. **Leave favourites** keeps favourites as they are.
- B41 only and B42 only badges, "Needed by" counts, update badges, and where each mod came from (Subscribed, Workshop, GitHub, Local, Manual).
- Asks before you disable a mod others need: **Disable them too** or **Disable, leave the rest on**.
- **Undo** and **Redo** (Ctrl+Z / Ctrl+Y) for every list edit.
- Right-click a mod: top or bottom of the load order, **Copy mod ID**, **Show in Maps**, **Pin its place in the load order**, favourite, hide.

### The load order (right list)
- Mods load top to bottom; the lower one wins when two change the same file.
- **Autosort**: turns on what your mods need, puts frameworks first and translations last, satisfies every `require`, `loadModAfter` and `loadModBefore`, and keeps your order otherwise. Ctrl+Z undoes it.
- **Sort A-Z**, moving mods only as far as their rules need.
- **Fix load order** enables required mods and orders mods by their `require`, `loadModAfter` and `loadModBefore` rules.
- A pinned mod stays between its neighbours through Autosort.
- **Overridden** shows mods whose files another mod replaces, and which one wins.
- Turning a mod on places it after the mods it needs.

### Mod details
- Author, version, needed game version, what it requires (enabled, disabled, not installed), what needs it, shared files and the description.
- **Update now** when there's a newer version, and **What changed since your version** with the change notes.
- **Restore previous version** after any update.
- **Load rules**: load first or last (of the list or its category), load after, load before, incompatible with, sorter category. Saved in Mod Load Order Sorter's `sorting_rules.txt`, so Autosort and that mod both follow them.
- **Your tags** on any mod.
- Workshop or GitHub page, **Show files**, and for Steam subscriptions **Unsubscribe on the Workshop page**.

### Updates and removing
- **Check for updates**, then **Update {count} mods**. Or check on start, or every 1 or 3 hours, and either tell you or install.
- Updating a mod your latest save uses asks first.
- PZMM only removes mods it installed. Steam subscriptions and mods you put in by hand are left alone.
- Removed mods go to the Recycle Bin (Trash on Linux), and it can also remove requirements nothing else uses.
- Changes made in the game's own mod menu are picked up, never saved over.
- Build 42 empties its mod list once after some updates. PZMM writes the marker that stops it, and if the game wipes the list anyway, PZMM keeps yours and puts it back when you launch from PZMM.

### Launching
- **Launch game** starts the game through Steam, after a check. Problems get **Fix** or **Launch anyway**; minor notes get **Launch, and don't ask about these again**.
- Warns when your latest save was played with a different mod list, with **Use this save's mods**.
- The activity log shows installs, updates and errors with times, and **Copy log** (your user name hidden). Its bar shows the job running; expand it to filter the full log.
- **Download history**: every install, update, removal and Workshop subscription, with reinstall. The last 500 are kept.

## Profiles

- One profile menu, grouped into Project Zomboid Profiles, Project Viewpoint Profiles and Server Profiles.
- **New profile…** starts empty; **Duplicate…** copies one; **Rename…** and **Delete…**. Right-click any profile to switch, rename, duplicate or delete.
- **Remove unused server profiles…** clears profiles for servers that aren't in your history, favourites or hosted list.
- **Share**: **Copy a share code**, **Save as a preset in the game's mod menu**, or **Save as a Mod Manager preset**.
- **Import**: a share code, Workshop collection or item links (several at once), a preset from the game's mod menu, or a list saved on this computer (Mod Manager and Mod Manager: Server lists). Missing Workshop mods get installed.
- Importing a collection asks: a new profile or add to this one, only the items you already have, which mods of a multi-mod item to turn on, and lists items that are gone from the Workshop.
- Reads [B42] Mod Manager's favourites and hidden lists, and can keep favourites on and hidden mods off in every profile.
- **Snapshots…**: the last 10 lists from before installs, updates, Autosort, grouping, removals, imports, arranging maps and the broken-mod search. **Restore** one; the list it replaces becomes a snapshot too.
- Each profile can have its own game arguments and a game folder of its own, so a server's profile can't touch your worlds.

## What's new

- Your Workshop mods updated since you last played (or since your last save), with every change note.
- **Latest updates** or **All change notes**, **Enabled only**, **Refresh**, and **Update** on each.
- "May break saves" on map mods, new major versions and notes that mention a new game or saves.
- Mods with no Workshop notes show their own changelog instead.

## Browse Workshop

- The whole PZ Workshop in the app, no Steam login.
- A **Home** view: Popular this week, Most subscribed, Newest, Recently updated, Most required.
- **Browse**: search titles, descriptions or both, with Steam's search operators ("exact phrase", AND, OR, NOT, +, -).
- Filter by game build, mods or collections, posted and updated dates, and **Only mods marked incompatible**.
- Tags: click to require, again to exclude.
- Sort by most popular (today to a year), most subscribed, top rated, newest, recently updated, most required or in the most collections.
- **Hide installed**, results per page, paging, and **Back** (Alt+Left).
- An item panel: screenshots and videos, rating, subscribers, size, dates, the description, **Change notes** and **Comments**.
- Mods that need it, collections it's in, more by the same author, and what it needs (installed with it).
- Collections with **Install all {count} mods…**.
- **Add to server** puts a mod and its required items on a server PZMM hosts.
- In a Viewpoint profile: **Viewpoint add-ons** and **Viewpoint Discord**.
- Workshop pages open in the Steam client when it's running, or in the browser.

## Maps

- The world's cell grid for Build 42 or 41: grey for vanilla, teal for the river, blue for a map, gold for a map over a vanilla area, red for two maps fighting over a cell.
- **World map** draws water, roads and buildings from each map's own world map under the grid. Or use your own image and **Calibrate** it from two spots.
- Zoom, pan, **Fit**, **Hide vanilla** and **Per-map colours**. Click cells to list the maps there.
- **Workshop maps you haven't installed**: every Map-tagged Workshop item (about 2,900) with its cells, from a built-in index, then only items updated since are read from Steam. Nothing is downloaded.
- Tick a map to preview it on the grid without changing your mod list. **Enable {count} previewed** when you're happy.
- **Most maps that fit** previews the largest set that shares no cell with each other or your enabled maps.
- Each clash shows which cells, with **Swap** to try the other map.
- Search by name, mod ID, Workshop ID, folder, author or tag. Filter to conflicting or over-vanilla maps; sort by cells, conflicts, subscribers, updates or newest.
- **Hide spawn-point mods** leaves out mods that only add spawn points to a vanilla town.
- **Requirements**: what the shown maps need, as a list or a tree, with find and install.
- **Order & export**: the map order (drag, Alt+↑/↓, **Autosort maps**), saved per profile and written to the game. **Copy {count} maps to the clipboard** gives Workshop links, mod IDs and the `Mods=`, `WorkshopItems=` and `Map=` lines.
- **Presets** of shown maps.
- **Compare** with a server config's `Map=`.
- **Check a map**: paste a Workshop link or a whole collection to see where it sits before installing.
- Flags duplicate mod IDs and map folders added twice.

## Servers: join

- **Join a server**: the Internet list, Favourites, History, Friends and **Direct connect** (IP:port or a `pzmm://join` link; nothing is contacted until you press **Look up**).
- On Windows, the Internet list comes through your running Steam, no key needed. On Linux, with Steam closed, or for friends' servers, add a Steam Web API key in Settings.
- If Steam's list doesn't load, the game's own saved list shows instead.
- Filter by build, mods (any, only mods I have, at most N to download), map, players, password, PvP and country. Sort by players, ping or name.
- Server details: **Mods** (installed, on the Workshop, probably, not found), **Rules**, **Players** and **Your joins**. A **BattleMetrics** link.
- **Set up and join**: a popup lists every mod with its Workshop image and size, marks probable matches as **Best guess** (untick to skip), then **Create profile and download** makes a profile of its own and starts the game connected. Your other profiles stay as they are.
- **Join as is** skips the setup.
- Mods the server marks **Server only** or **Optional** start unticked.
- Warns when a mod was updated on the Workshop after the server last restarted, the usual cause of "version is different".
- Warns when your build doesn't match the server's, and when its maps overlap or need a map mod it doesn't list.
- Servers run from PZMM carry their exact mod list, load order and maps in their description. Other servers are matched through a mod ID index of the whole Workshop that ships with PZMM.
- History keeps the setup you joined with: **Use the setup from {date}** brings it back.
- Passwords can be remembered in the system's credential store.
- Optional: server countries (only the servers' addresses go to api.country.is), and a notification when a favourite server comes online with room.

## Servers: host

### Setting one up
- **New server…**: name, build (Build 42 or Build 41 `legacy41`), install folder, data folder, ports (**Check ports**, plus the firewall command to copy), max players, join password, public listing, admin password, memory with guidance from your RAM, and mods from a profile or a collection.
- **Set up and start**: SteamCMD installs the server anonymously (about 2 GB) and starts it once to make the world.
- UPnP stays off unless you turn it on. Every Workshop ID is checked before it's written, because a bad one crashes the server.
- No port forwarding? Paste a tunnel address (such as playit.gg) and PZMM shows it where players need it.
- **Adopt** configs already in `Zomboid/Server`, **Find existing servers** for LinuxGSM and Docker installs, and **Add a remote server…** over RCON.
- **Copy settings** makes a new server from this one with a fresh world.

### Running it
- Start, Stop (saves first), Restart and **Join**. **Warn players, stop in a minute** and **Warn players, restart in a minute**, with **Call off**.
- Status, players, CPU and memory over the last 15 minutes, uptime and next restart.
- **Update server** with SteamCMD, or switch build.
- When you quit PZMM with a server up: **Stop and quit** or **Leave running and quit** (PZMM finds it again over RCON).
- Keep PZMM in the tray so schedules and crash watch keep going with the window closed.
- **Copy diagnostics** for a bug report, passwords hidden.

### Console
- Live log with errors, warnings, chat, admin and mod lines picked out, and errors mapped to mods.
- Chat and perk logs.
- A command list with forms: teleport, give items and XP, spawn vehicles and hordes, god mode, invisible, noclip, weather, lightning and time speed.

### Players
- Online, Characters (everyone in the world, with where they are), Map, Whitelist, Bans (account, SteamID, IP) and History.
- Kick, ban, set access level, mute voice chat, notes and tags on each player.
- Messages to everyone, with a preview of how players see them.

### Mods
- The server's mod list as a real list: Workshop titles, search, drag order, **Autosort**, **Check IDs**.
- Who needs each mod: Server and players, Server only or Optional.
- Required Workshop items added for you. Spawn points offered for new map mods.
- From a profile, a collection or a Mod Manager: Server preset; **Save as a preset**; **Edit as text**.
- **Copy the lines** for a rented server's panel.
- **Write…** shows the changes first and keeps the original as `.ini.bak`.
- **Mod list in the description** so PZMM players get an exact match.

### Settings
- The `.ini` as grouped forms with the server's own descriptions and defaults.
- The sandbox, base game and mod options apart, with presets shared with the game's Sandbox Presets folder.
- Spawn regions, a text editor, and **Restore previous**.

### Automation
- When a mod updates: restart after warnings (for example 15, 5 and 1 minutes), restart once nobody is online, restart at the next restart time, or only tell you.
- Daily restart times, restarts every N hours, scheduled saves, backups and messages to players.
- Crash watch restarts a crashed server a set number of times an hour, then stops and tells you.
- A history of the last 24 hours.
- **Discord**: webhook alerts (started, stopped, crashed with the last log lines, restarting, mods updated, joins and leaves) and a status message kept up to date.
- Discord bot commands for a role you pick: status, players, restart, save, msg, update-check and help, typed like `!status`.
- **Relay chat**: the game's general chat goes to the bot's channel, and messages there show in game as [Discord] messages.

### Backups and world
- **Back up now**, before each start, before updates and on a schedule, with a size and count limit.
- **Restore** in one click (the current save is backed up first). **Export** and **Import** backup zips.
- Warns about empty map files after a crash.
- **Wipe…** the world, keeping characters and safehouses if you like.
- **Reset map areas** by tile coordinates, inside or outside the areas you give.
- **Move in a co-op world** you hosted from the game.
- Back up and restore only the safehouses, zones and factions.
- **Save a start script…** to run the server without PZMM.

### Files (FTP)
- For rented servers: connect over FTP (TLS by default), **Get settings files**, edit them in the Settings tab, **Send settings files** (the host's copies are backed up first), and **Back up the world**.
- A self-signed certificate can be trusted by its fingerprint. A changed one is refused until you confirm it.

### Sending a profile to a server config
- Write the active profile's mods, Workshop items and maps to a server `.ini` (**Write to {name}.ini**), with a preview. Other settings stay.
- **Use as profile** makes a profile from a server config and installs its missing mods.

## Saves

- Every save with its last screenshot and game mode.
- Backups when PZMM starts the game, when the game closes, every N minutes while playing and after a crash.
- **Back up now**, named **Save slot**s that are never trimmed, **Keep as a slot**, **Copy as a slot**, **Restore** and delete. A backup folder of your choice.
- Deleted saves that still have backups can be brought back.
- **Give it my mods** shows what changes first and keeps the old list as `mods.txt.prev`.
- **Copy with my mods** makes a new save, named with `_modded`, and leaves the original alone.
- **Sandbox**: edit an existing save's sandbox settings, with presets.
- **Recovery**: **Repair** removes empty map files; **Reset** the zombie population, the clock, calendar and weather, or the animals (Build 42). Each backs up first.
- **Characters**: who's in the save, where, alive or dead. Read-only.
- **Data**: browse the save's databases, read-only.

## Java mods

### ZombieBuddy
- [ZombieBuddy](https://github.com/zed-0xff/ZombieBuddy) by Andrey "Zed" Zaikin (zed-0xff) is the Java agent that lets mods ship Java code. PZMM sets it up; it isn't part of PZMM.
- A checklist: mod downloaded, agent files in the game folder, launch flag in `ProjectZomboid64.json`, enabled in this profile.
- **Set up ZombieBuddy** lists every change, then **Apply changes**. Works with a game on any drive, and on Linux.
- Game updates and Steam's "Verify integrity of game files" remove the flag. PZMM notices, puts a red dot on the tab and offers **Repair ZombieBuddy**.
- **Remove from game folder** takes it all back out.
- Java mods you turn on in PZMM are approved in ZombieBuddy when you press Play, so it doesn't ask again in the game. An update brings a new jar, which PZMM approves on the next Play. Jars with scan warnings, or not from the Workshop, still ask. A "no" you gave in ZombieBuddy's own dialog stays, and Check setup offers to allow it again.
- People on the alternate launcher or Steam launch options get pointed to the official installer.

### Project Viewpoint
- [Project Viewpoint](https://steamcommunity.com/sharedfiles/filedetails/?id=3809306528) by ellu and norkus adds first- and third-person play to Build 42. It's their project, early in development, and not affiliated with PZMM. Viewpoint bugs go to the [Viewpoint Discord](https://discord.gg/g83XvPhEgQ).
- **Set up Project Viewpoint…** in the profile menu (or **Install and set up** here) downloads Viewpoint and ZombieBuddy from the Workshop, sets up ZombieBuddy and makes a Viewpoint profile from your current one, with both at the top. You see each change first.
- **Switch to Viewpoint** here, or pick a profile from the other game in the profile menu to switch back and forth. Each side keeps its own mod lists. ZombieBuddy stays in place when you switch back.
- **Update Viewpoint** when the Workshop has a new version.
- Not offered on Build 41.

### Java scan
- Every Java file your enabled mods bring is hashed, its signature checked, and scanned before ZombieBuddy loads it: network, starting programs, files, native code, encryption, hidden code.
- Mod folders, Lua, the game folder and launch options are checked too.
- Shows whether ZombieBuddy will load, block or ask about each file. **Copy the SHA-256**.
- Finds other loaders (Leaf, Avrix) and scans what they load.
- **Looked at it** silences a warning until the file changes.
- Before launch, new or changed Java files get **Review Java mods**.

## Check setup and logs

- **Check setup** checks the profile and server configs: missing, disabled or late requirements, incompatible mods, wrong-build mods, Lua `require`s that point nowhere, Lua syntax errors, mod.info problems, duplicate mod IDs, game version limits, overlapping maps, mods in two places, Workshop items that are gone, hidden or unlisted, and repacks of another author's mod.
- Server lines: missing Workshop items, wrong mod ID case, missing backslashes on Build 42, maps after Muldraugh, and a build different from yours.
- Problems are sorted Broken, Risk, Check and Low, each with its fixes (download, turn on or off, sort, switch build, uninstall, find a Build 42 version and more). One button runs all the harmless fixes. **It's fine** hides a finding until that mod changes.
- A Java file you refused in ZombieBuddy's dialog gets **Allow it again**.
- Lua errors from the last game session, counted per mod.
- The same check runs before **Launch game**, **Join** and starting a server.
- **Read the log**: the game's and servers' logs, each error once with its trace and the mods it points at, with search, errors only, **Follow** and **Show in log**.
- **Find the broken mod** halves your enabled mods each time you test: **The problem still happens** or **The problem is gone**, until it names the mod. It starts with the mods the log blames. **Stop and restore my list** at any point, and **Go on searching** if you left one unfinished.

## Make a mod

- **Choose folder…** checks a mod, or an upload folder, the way the game and the Workshop uploader will: mod.info keys, the Build 41 and 42 layouts, version folders, refused file types, the preview image, `Contents/mods/`.
- **Build upload folder** writes `workshop.txt` (title, description, tags, visibility, item ID), a 256×256 preview from your poster and a clean copy of the mod.
- Tells you if the folder isn't where the game's uploader looks, and what to click in the game next.

## Settings and app

- **Folders**: the game, Zomboid folder, dedicated server, Steam Workshop, PZMM data and save backups, with **Browse…**, **Detect** and **Open**. Warns about OneDrive, write access and low disk space. **Move data** to another drive.
- **Downloads and updates**: mod update checks, where Workshop pages open, PZMM's own update check, and **Re-download mods PZMM installed**.
- **Accounts and keys**: a GitHub token and a Steam Web API key, both kept in the system's credential store.
- **Removing and backups**: Recycle Bin or delete, removing unused requirements (Ask, Always, Never), a size cap for mod backups, **Clear backups**, and save backup settings.
- **Appearance**: language, theme (dark, light or match system), accent colour, game build (or detect from `default.txt`), what the Workshop tab opens on, results per page, **Compact rows**, **Show mod IDs beside names**, favourites at the top.
- **Launch and notifications**: extra game arguments, per-profile arguments and game folder, grouping a new profile by category, keeping favourites on and hidden mods off, the tray, what the close button does (ask, minimise to the tray or quit), starting in the tray at sign-in, desktop notifications (server crashes, restarts needed, finished downloads, favourite servers online), and server countries.
- **Performance**: the game's memory with a suggestion (**Set**), Java options and garbage collector, PZ_Optimization **Install**, **Update** and **Uninstall**, and the mods that slow loading most.
- **About**: version, **Check for updates**, **Open log folder**, **Copy diagnostics** (user name masked) and the keyboard shortcuts.
- PZMM updates itself (**Update and restart**), with signed updates.
- `default.txt` keeps the format it already has (Build 41 or 42); the previous one is kept as `default.txt.bak`.
- 10 languages: English, Deutsch, Español, Français, Italiano, 한국어, Polski, Português (Brasil), Русский and 简体中文.
- Every confirmation appears inside the app. Writes are atomic, and files PZMM can't read are left alone rather than rewritten.
- `pzmm://join` links open the server in Direct connect.

## Command line

The `pzmm` command line (a single file, for servers without a desktop) shares profiles and data with the app. `--json` works on the read commands.

- `pzmm add` (Workshop ID or link, `owner/repo[@ref]`, zip or folder; `--no-enable`), `list`, `enable`, `disable`, `remove`, `update`, `rollback`
- `pzmm profile list | new | use | rename | duplicate | delete | export | import`
- `pzmm snapshot list | restore <id>`
- `pzmm build 41|42` forces the `default.txt` format
- `pzmm search` (with `--sort`, `--build`, `--page`), `pzmm find <mod id> --install`, `pzmm outdated`, `pzmm news`
- `pzmm doctor` (exit code 1 on errors), `pzmm errors`, `pzmm check-mod <folder>`
- `pzmm mode zomboid|viewpoint` (`--dry-run` lists the changes), `pzmm zombiebuddy status | install | uninstall`
- `pzmm server create | start | stop | status | console | log | backup | backups | export | import <name>`; `create` takes `--build legacy41`, `--dir`, `--data`, `--port` and `--memory`. `start` runs the server's schedules, update checks and crash watch while it runs.
- `pzmm servers` lists the servers PZMM manages or found
- `pzmm watch <server> --every 10 --on-update "<script>"` runs your script when the server's Workshop items update
- Only in the app: the server browser and joining, Maps, browsing the Workshop, save tools, and setting up a server's schedules, Discord and FTP.
