# PZMM 1.3: the companion mod

Status: planned · 2026-10-11. Follows [ROADMAP-1.2.md](ROADMAP-1.2.md) (done).

## Goal

A PZMM companion Workshop mod that replaces the game's own mod screen and the in-game mod managers ([B42] Mod Manager, Mod Manager), works on its own, and does more when the PZMM app is installed. It's the in-game half of PZMM; the app keeps everything the game's Lua can't do.

## What goes into the game

| Area | In game | How |
|---|---|---|
| Mod list: enable/disable, search, filters, favourites, hidden, tags | Fully | Lua replacement of the mod screen (the vanilla one is Lua too) |
| Load order, Autosort, Load Order Sorter rules, "needed by" warnings | Fully | port of `app/src/lib/order.ts` to Lua |
| Map order | Fully | as the game's own mod screen does |
| Presets and PZMM profiles | Fully | the app mirrors profiles into `Zomboid/Lua/pzmm/`, which game Lua can read |
| Requirement, build and incompatibility warnings | Mostly | from mod.info; deeper findings from the app's last Check setup via the bridge |
| What's new, update alerts, the first real error after a crash | Shown in game | computed by the app, shown in the main menu |
| Sandbox options of the current save | Fully | in-game, like Change Sandbox Options |
| Switch profile and restart into it | With the app | the mod writes a request; the app applies it and relaunches |
| Installs from GitHub/zip, SteamCMD, updates and rollback, hosting, server launcher, Java scans, self-update | No | game Lua can't download, run programs or read binary files |

## Queue

1. [ ] **API check (first):** on 42.21 (and legacy41), list what game Lua can reach: the file reader's roots, Workshop item queries and subscribe calls, the mod list and map order APIs the vanilla mod screen uses, restart/quit to main menu. Decide the bridge format from what's readable.
2. [ ] **Bridge:** the app writes `Zomboid/Lua/pzmm/` (profiles, findings, news, error summary, app version) and reads requests the mod leaves there; versioned, small, never required.
3. [ ] **Mod screen replacement:** list, filters, favourites/hidden (shared with the game's and [B42] Mod Manager's files), tags, presets, map order.
4. [ ] **Load order in Lua:** Autosort and Sorter rules with the same results as the app (shared test cases).
5. [ ] **Warnings and news in the main menu:** requirements, build tags, the app's findings, What's new, the last crash's first error.
6. [ ] **Profiles from the game:** pick a PZMM profile, apply it, restart into it.
7. [ ] **In-game sandbox editor** for the current save (admins on servers), sharing presets with the app.
8. [ ] **Workshop listing** (public: needs the owner's go-ahead), B41 and B42 layouts, translations from the app's locales.
