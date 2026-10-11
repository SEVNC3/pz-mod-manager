# 1.2 work queue

From [ROADMAP-1.2.md](ROADMAP-1.2.md). The 1.1 queue is done ([RELEASE-1.1-QUEUE.md](RELEASE-1.1-QUEUE.md)). Ordered by value: each phase is usable on its own.

## R0: ship 1.1 first

1. [ ] Decide: squash the GIF add/remove commits; noreply email for commits.
2. [ ] A short real-data pass on the dev build (Maps, Settings, load order on your own profile), then push, CI, tag v1.1.0.
3. [ ] Signed releases and an "official download" line in README and the app's About (GitHub has junk repos using the PZMM name).

## V: verify before building the launcher

4. [ ] Capture `A2S_INFO` / `A2S_RULES` from a few public modded servers (B41 and B42): exactly which keys, how mods are listed, truncation, description fragments. Record fixtures for tests.
5. [ ] Check on a real 380870 install: does the server read `-Xmx` from its own `ProjectZomboid64.json`; RCON's default port on B42; where backups go; what `checkModsNeedUpdate` prints.
6. [ ] Find whether the game's join handshake exposes the server's Workshop IDs (for layer 3 of the launcher design).

## S1: host a server from PZMM

7. [ ] **Setup wizard**: name, build (B42 or legacy41), install folder, then SteamCMD installs app 380870 anonymously. Next: ports (checks they're free and shows the firewall rule), admin user and password, join password, max players, public listing, memory (from RAM, with the hosting guidance as hints), map and mods (from a profile, a collection or empty). Then the first start.
8. [ ] **Server page**: one card per server with status (stopped, starting, running, players, uptime, memory), Start / Stop (saves first) / Restart, and a **live log** (tail of the server console, filterable, errors highlighted and mapped to mods).
9. [ ] **Console**: send commands over stdin (PZMM-started) or RCON (already running or remote). Command palette with the admin commands and arguments, and history.
10. [ ] **Players**: online list (`players`), kick, ban, set access level, message, add user, whitelist.
11. [ ] **Mods for the server**: edit its Mods / WorkshopItems / Map from a profile (the library and load order UI), validate Mod ID ↔ Workshop ID and the B42 `\`, dry-run diff, write with a backup.
12. [ ] **Keep mods current without breaking joins**: `watch` sees an update → countdown warnings in game (15/5/1 min via `servermsg`) → `save` → `quit` → SteamCMD refresh → start. Options: only when empty, or at a set time.
13. [ ] **Schedules**: restarts at set times with warnings, backups on a timer, update checks.
14. [ ] **Backups**: on start, before updates and on a schedule, with retention and one-click restore. Detects zero-byte cell files.
15. [ ] **Config forms**: the .ini (grouped, with descriptions and defaults read from the installed server), sandbox settings with presets (option list read from the game, so it follows new versions), and spawn regions.
16. [ ] **Discord webhook**: started, stopped, crashed, restarting soon, mods updated, player joined or left (each optional), with a status message kept up to date.
17. [ ] **Crash watch**: restart on crash, with the last log lines in the alert.
18. [ ] **Existing servers**: detect servers in `Zomboid/Server`, LinuxGSM installs and the common Docker images; manage their configs and mods even when PZMM doesn't run them.

## S2: find and join modded servers (the launcher)

19. [ ] **Server browser**: list (with a Steam Web API key from Settings, or a PZMM proxy if we decide to run one), then A2S each server for players, ping, version and mods. Filters: build, mods (any / only ones I have / at most N to download), map, players, password, PvP, region. Favourites, history, search.
20. [ ] **Server details**: description, rules, players, mod list resolved to Workshop items (installed, missing, size), map list.
21. [ ] **Set up and join**: diff → download missing (SteamCMD or Steam) → a per-server profile with the server's load order and maps (your profiles stay untouched) → launch with `+connect ip:port +password`. **Join as is** skips the sync.
22. [ ] **Mismatch pre-flight**: warn "N mods updated on the Workshop after this server last restarted; joining may fail until it restarts", with the times.
23. [ ] **Join history** keeps the exact mod set used per server, so a known-good setup can come back.
24. [ ] **Direct connect** by IP:port and from a `pzmm://join/…` link, which Discord announcements can use.

## S3: exact mod sync between server and players

25. [ ] **Mod list in the server description**: servers run from PZMM add a short PZMM code to the public description (Workshop IDs, mod IDs, load order, map order, Workshop update times). Players' PZMM reads it from `A2S_RULES` and matches exactly. A manual "copy the tag for my host's panel" covers rented servers.
26. [ ] **Mod ID index for every Workshop item**, built like the map seed (file lists plus mod.info through `steamroom`), shipped as a seed and refreshed incrementally. It maps any server's mod IDs to Workshop items when there's no PZMM code.
27. [ ] **Pinned versions**: record each item's Workshop update time with the list, and say clearly when a mod has moved on since the server's list was made.

## A: absorb the rest (mostly independent; any order)

28. [ ] **Pre-launch analyser**: build tags vs your build, missing requirements, declared incompatibilities, duplicate mod IDs across items, mod ID ↔ Workshop item, Lua `require` targets that don't exist, mod.info problems. Shown in Check setup and before Launch.
29. [ ] **Log intelligence**: read `console.txt` and server logs, find the first real error, map Lua paths and Workshop IDs to the mod, and offer Find the broken mod starting from the suspects.
30. [ ] **Changelog feed**: mods updated since you last played, with their change notes, in one list (the "What changed" in Details is per mod today).
31. [ ] **Collections → anything**: paste a collection to make a profile, a server's mod lines, or a Maps preview, with IDs read from mod.info (no "unresolved" IDs) and the B42 prefix. Includes a bundled sandbox preset if the pack ships one.
32. [ ] **Import and export the Workshop managers' lists**: Mod Manager `saved_modlists.txt` and Mod Manager: Server `saved_modlists_server.txt`, both ways.
33. [ ] **Load Order Sorter rules**: read `sorting_rules.txt` and its mod.info keys (loadFirst, loadLast, incompatibleMods), and write rules when the player pins an order.
34. [ ] **Save tools**: backups and named slots for single-player saves (on launch, on exit, by hand), restore, safe edit of a save's mods with a diff, broken cell file detection. A read-only `players.db` viewer.
35. [ ] **Sandbox editor for existing saves** (`map_sand.bin`), with the same presets as servers.
36. [ ] **Library folders and tags**: auto-tags (framework, patch, map, UI, translation), filters for disabled, hidden and favourites, search by author. Reads [B42] Mod Manager's hidden and favourite lists.
37. [ ] **Java mods**: hash and signature per ZombieBuddy mod, a simple bytecode scan for dangerous calls (network, process, file outside the game), shown before ZombieBuddy is asked to load them.
38. [ ] **Performance panel**: memory setting for the game (ProjectZomboid64.json) with advice, PZ_Optimization detect/install with a revision check, mod count and load-time hints.
39. [ ] **Authoring helpers** (last): mod.info validator, B41 → B42 folder layout check, `workshop.txt` and upload-folder builder.

## Notes

- Release batching still applies: build and commit locally; push and tag only when you say.
- Every server action that changes files or stops a server confirms first, and keeps a backup.
- Network politeness: one Steam session, backoff on 429/503, cache A2S results; no scraping of sites whose terms forbid it.
