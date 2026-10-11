# PZMM 1.2: the all-in-one for Project Zomboid modding

Status: research done, nothing built · 2026-10-11. The work is in [QUEUE-1.2.md](QUEUE-1.2.md).

## Positioning

One app for everything a player or a server owner does with mods:
- **Players**: find, install, order and fix mods; pick a modded server, get exactly its mods, and join.
- **Server owners**: set up a server, run it with a live console, keep its mods current without breaking joins, back it up, and publish a mod list players can match in one click.

Today that takes a stack of Workshop mods (about 10M subscriptions between them), hosting companies' web tools, and a few small GitHub projects. Desktop rivals barely exist: the standalone PZ managers on GitHub have 0-7 stars each, and the closest all-in-one (Unjammer/PZ_Launcher) has 3.

## What the research found

**Servers** (sources in the research notes below)
- Build 42 multiplayer is stable (42.20 on 2026-07-29, 42.21 on 2026-09-28; up to 254 players). B41 is the `legacy41` branch.
- The dedicated server is Steam app **380870** and installs anonymously through SteamCMD, which PZMM already ships.
- Configs live in `Zomboid/Server/<name>.ini` with `_SandboxVars.lua`, `_spawnpoints.lua` and `_spawnregions.lua`. Ports are UDP 16261 and 16262. RCON is Source RCON on `RCONPort`, off while `RCONPassword` is empty.
- Admin commands (`players`, `servermsg`, `save`, `quit`, `kickuser`, `banuser`, `checkModsNeedUpdate`, …) work on the console's stdin or over RCON.
- **The top server pain is the version mismatch**: Steam updates clients' mods straight away, but a server only fetches Workshop updates when it starts, so joins fail until someone restarts it. `pzmm watch` already detects the update; the missing piece is warn, save, quit and relaunch.
- **Second pain**: the Mod ID and Workshop ID lists must stay aligned, and B42 wants `\` before each mod ID. Old lists fail silently.

**Discovery and joining**
- PZ servers answer Steam's A2S queries on the game port. `A2S_RULES` carries the version, PvP and open flags, the description, and the **mod IDs** (semicolon-separated, sometimes truncated, with a `modCount`). It does **not** carry Workshop IDs.
- The Steam master list now needs a Web API key, so PZMM needs a user's key or a small proxy for a full list. BattleMetrics has an API that needs a token and has terms to check.
- The client joins with `+connect ip:port +password pw`. The game's own download on join is unreliable (stalls, "Getting Server Info…", version mismatches).

**Other launchers** (DayZ, Arma 3, Minecraft and others)
- DZSA and the Arma 3 launcher: pick a server, see its required mods, then "set up mods and join" downloads the missing ones and launches.
- Arma3Sync and packwiz: the server owner publishes a manifest at a URL, and clients sync to it exactly on every launch.
- r2modman and Arma presets: profiles shared as a code or file.
- LinuxGSM, Pterodactyl, AMP and txAdmin: install wizards, live console, scheduled restarts with countdown warnings, update-then-restart, backups with retention, Discord webhooks, and "only stop when no players are on".

## The design that makes PZMM the launcher

The hard part is turning a server's mod IDs into Workshop items in the right order. PZMM can solve it in three layers, best first:
1. **A PZMM code in the server description.** A server run from PZMM puts a short tag in its public description (it shows in `A2S_RULES`). The tag holds a share code: Workshop IDs, mod IDs, load order and map order, and the Workshop update times it was built with. Players get an exact match with no PZMM service in the middle. This works on any host, because it's only text in the ini.
2. **A mod ID index.** PZMM already reads Workshop file lists anonymously (`steamroom`) and ships a seed of every map. The same scan over every Workshop item gives mod ID → Workshop item for the whole Workshop, kept current the way the map seed is.
3. **Ask the game.** As a last resort, connect to the server once and read the Workshop list it sends, if the protocol allows (investigate).

The join flow is the same in all three: compare the server's list with what's installed → show what's missing and its download size → download it with SteamCMD (or Steam) → apply a per-server profile, so your own profiles are untouched → launch with `+connect`.

## What PZMM can replace

| Today | Demand | PZMM feature | Verdict |
|---|---|---|---|
| Mod Manager / Mod Manager: Server (Workshop) | 4.26M / 2.38M subs | Profiles (done); import and export their `saved_modlists*.txt` | Absorb |
| errorMagnifier and log viewers | 3.5M subs | Log intelligence: first real error → mod; feeds Find the broken mod | Absorb |
| Mod Update and Alert System, Workshop Update Checker | 2.17M / 170k | Changelog feed: updated since you last played, with notes | Absorb |
| Change Sandbox Options, Sandbox Preset Saver, web sandbox generators | 1.2M / 30k | Sandbox editor for servers and existing saves, presets, option list read from the installed game | Absorb |
| Mod Load Order Sorter (+ its rules file) | 1.08M | Autosort (done); read and write its `sorting_rules.txt` | Absorb + integrate |
| Better Server Settings, No More Modded Server Start Problems, Mod Checker | 380k / 134k / 192k | Server mod list builder with mod ID ↔ Workshop ID validation | Absorb |
| Load From Collection; web "mod ID grabbers" (Winternode, DoomHosting, Supercraft) | 365k + host traffic | Paste a collection → profile or server config, IDs read from mod.info, B42 prefix | Absorb |
| Server update checkers / auto-restarters (several mods), PZServerDiscordBot | ~60k subs, 97 stars | Watch → warn → save → quit → update → relaunch, with Discord webhooks | Absorb |
| zomboid-control-panel, pz-admin, ZomboidRCON, kldzj/pzmod, Windows server managers | 125 / 29 / 36 / 34 stars | Server tools: wizard, console, players, schedules, backups, config forms | Absorb |
| Save backup tools and mods (PZSaveManager, DataBackup, Slot Manager, World Recovery) | 18k-ish each | Backups and named slots for saves and servers; broken cell file detection | Absorb |
| Vanilla Mod Manager (safe save edits) | 26k | Diff and confirm before changing a save's mods (partly done) | Absorb |
| Error View, Mod Checker (static checks) | 10k / 192k | Pre-launch analyser: build tags, missing requirements, conflicts, duplicate IDs, Lua `require` | Absorb |
| PZ-ModGuard, Simple Mod Check (Java mods) | small, new | Java mod hashes and a malware scan on top of ZombieBuddy | Absorb |
| PZ_Optimization (class-file patch) | 74k subs | Performance panel: detect, install, warn on game revision mismatch; memory advice | Integrate |
| Docker images, LinuxGSM `pzserver` | 386 / 238 / 182 stars | Detect and write their configs; templates | Integrate |
| Map Mod Manager | 488k subs | Maps tab (done) | Absorbed |
| [B42] Mod Manager (in-game) | 480k | Keep compatible: read its hidden and favourite lists | Integrate |
| Authoring tools (TileZed, WorldEd, Umbrella) | n/a | Out of scope; only mod.info validation, B41→B42 layout check, Workshop upload folder | Leave (helpers last) |

Also worth doing: signed releases and an "official download" notice. GitHub already has junk repos using the PZMM name.

## Research notes

Key sources:
- Servers: pzwiki [Dedicated server](https://pzwiki.net/wiki/Dedicated_server), [Startup parameters](https://pzwiki.net/wiki/Startup_parameters), [Admin commands](https://pzwiki.net/wiki/Admin_commands); [LinuxGSM pzserver config](https://raw.githubusercontent.com/GameServerManagers/LinuxGSM/master/lgsm/config-default/config-lgsm/pzserver/_default.cfg); [version mismatch](https://supercraft.host/wiki/project-zomboid/workshop_item_version_is_different/); [A2S rules as read by zomboid-monitor](https://github.com/zarant77/zomboid-monitor); [Valve server queries](https://developer.valvesoftware.com/wiki/Source_Server_Queries); [Steam news for 42.20/42.21](https://store.steampowered.com/feeds/news/app/108600/?cc=US&l=english&count=10).
- Launchers: [dayz-ctl](https://github.com/WoozyMasta/dayz-ctl) (built on the DZSA API), [Arma 3 Launcher](https://community.bistudio.com/wiki/Arma_3_Launcher), [Modrinth pack format](https://docs.modrinth.com/modpacks/format), [LinuxGSM alerts](https://docs.linuxgsm.com/alerts), [txAdmin](https://github.com/citizenfx/txAdmin/blob/master/README.md).
- Tools to absorb: [zomboid-control-panel](https://github.com/fpsacha/zomboid-control-panel), [kldzj/pzmod](https://github.com/kldzj/pzmod), [PZ_Launcher](https://github.com/Unjammer/PZ_Launcher), [Winternode mod ID grabber](https://winternode.com/tools/project-zomboid/mod-id-grabber), [Mod Load Order Sorter](https://steamcommunity.com/sharedfiles/filedetails/?id=3423660713), [errorMagnifier](https://steamcommunity.com/sharedfiles/filedetails/?id=2896041179), [PZ-ModGuard](https://github.com/KodeMannn/PZ-ModGuard), [PZ_Optimization](https://github.com/xD3I/PZ_Optimization). Subscriber counts are from Steam's GetPublishedFileDetails on 2026-10-11 (lifetime subscriptions overstate active users).

Unverified and needing a check before building: a live `A2S_RULES` capture from a modded server; whether the server reads `-Xmx` from its own `ProjectZomboid64.json`; the B42 RCON default port; the Steam Web API server list method and its key terms; BattleMetrics' API terms.
