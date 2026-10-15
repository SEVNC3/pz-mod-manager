# In-game mod manager: action plan

From the first audit and six follow-up audits on 2026-10-15: three fine-grained (My mods, What's new and Home, other tabs) and three wide-scope (sync, game API capabilities, product and UX).

The rules behind it:
- The game does everything its Lua can, with or without the app.
- With both installed, everything syncs both ways, and the app unlocks more.

Sizes: S is under an hour, M is a session, L is several sessions.

## Phase 0: bugs to fix first (done 2026-10-15, not yet tested in game)

Three items were left:
- **Maps Autosort claim:** the notice is true until Phase 2 builds Autosort.
- **`fit()` and UTF-8:** no change needed, because Kahlua strings are Java strings, so `sub` never splits a character.
- **Copy and paste presets:** deferred to Phase 2 #3.

1. **Logged errors every frame without ZombieBuddy.** `drawJava` and `PZMMNews:prerender` call ZombieBuddy APIs that don't exist without it. Guard them with `if ZombieBuddy and ...` instead of relying on pcall.
2. **"Only X" turns off the dependants too.** `forceActivateMods(false)` cascades. Use `model:setModActive(id, false)` and then `refreshMods()`. Say why when turning a mod on fails (it can't load, or it's incompatible).
3. **Unsaved changes.**
   - Snapshot the active mods in `open()`, so Back with Don't save keeps the load order.
   - Count favourite changes; they're stored as strings, so compare as booleans.
   - Ask before a Rescan when changes are pending, because `reloadMods` resets them.
4. **Continue after Delete loads a stale save.** Refresh the main screen's latest save after a delete. Cache `getLatestSave()`, which currently runs every frame.
5. **The popup menu uses radius 8.** There are no corner textures at 8; use 6.
6. **Scroll and hover.** Clamp the list, feed and page scroll after anything shrinks them. Ignore hover and tips outside the clipped area. Reset the scroll when the UPDATES stat opens What's new.
7. **Dragging.**
   - A drop into a collapsed section unfolds it.
   - No dragging while a search or filter is on, as in the app.
   - Right-clicking mid-drag cancels the drag.
   - The ghost label is shortened to fit.
   - A `before` mod that isn't in the library is added first.
8. **Section edge cases.**
   - A section whose mods are all uninstalled is still shown.
   - Empty sections show in the All tab only.
   - Rename and Remove on "Other", and drops into it, work.
   - Removing an empty section doesn't leave an empty "Other".
9. **Library edits.**
   - Re-read `library.txt` before each edit, because the app may have rewritten it.
   - Create the game's own sections only on a real move, not on a click.
   - With the app installed but no library yet, say "Open PZMM once" instead of inventing sections.
10. **Launch and Play.** Clear `launchAt` in `open()` and `close()`. Play on a Multiplayer or Last Stand save, or a save that can't load, says why instead of closing silently.
11. **What's new with the app installed but no `feed.txt`.** Fall back to the Steam-only feed. Add a "Waiting for Steam" state, and a "no Workshop mods" state. Rescan asks Steam again.
12. **Wrong "app only" claims.**
    - The Saves notice lists "Give it my mods", which the game has.
    - The Home body.
    - The UPDATES stat says "App only"; show "..." until Steam answers.
    - The Maps notice lists Autosort.
13. **Small fixes.**
    - The Workshop page opens through `openLink` when there's no Steam overlay.
    - The details poster has a height cap, and isn't looked up again every frame.
    - The description wrap is cached.
    - `fit()` doesn't cut UTF-8 characters in half.
    - Sorting is case-insensitive with the mod ID as a tiebreak.
    - A profile name with ":" doesn't break Copy mod list.
    - The prompt has a maximum length.
    - Hide "Game's Workshop screen" without Steam.

## Phase 1: harden the bridge (built 2026-10-15, launch page published; not yet tested in game)

1. **`app.json` heartbeat** (S): replaces `app.txt`, written every 5 s with `proto`, `running_at`, active profile and mode. The game then knows whether PZMM is running and which profile is active, rather than guessing from `requests.txt`.
2. **Stamped requests** (S): every request carries a time, the profile and a base revision. The app drops loud requests older than 60 s, brings itself to the front at most once every few seconds, and ignores quiet requests meant for another profile.
3. **Acks and app-owned `library.txt`** (M): the app writes `acks.json` (each request applied or rejected), and the game's edits show at once and settle against it. `library.txt` becomes the app's alone to write.
4. **Sync gaps** (S each):
   - The app adopts the game's map order (the `Map=` line in default.txt) into its own.
   - It watches `default.txt` while it's in front.
   - It writes the library again on profile rename and delete.
   - It imports sections made in game when it's installed later.
5. **Zomboid folder mismatch** (S): `game.json` reports the game's real folder, and the app warns when it differs.
6. **Publish `site/open`** to GitHub Pages and test starting PZMM from the game.

## Phase 2: app features the game can do itself (built 2026-10-15, not yet tested in game)

Left for later: dragging maps (Up, Down and Top cover it), Autosort's loadFirst / loadLast from Mod Load Order Sorter, file overlaps in subfolders (the game lists only a folder's own files).

| # | Feature | How in Lua | Size |
|---|---|---|---|
| 1 | Load order editing: drag, Autosort, Fix | reorder `getActiveMods():getMods()`; `getRequire/getLoadAfter/getLoadBefore/getIncompatible` | L |
| 2 | Check setup in game, with fixes | game version against the mod's min/max, requirements, order rules, Build 42-only (`getModFileReader(id,"42/mod.info")`), Steam state, ZombieBuddy | M |
| 3 | Presets | `Model.presets`, `getPresetShareText`, `addSharedPreset`, `Clipboard.getClipboard()` (the file the app already reads) | S |
| 4 | Maps: Autosort, titles, Muldraugh last, Top | `map.info` `lots=` through `getModFileReader`; `getMapInfo(folder).title` | M |
| 5 | Saves: Recovery, Characters, Rename, mods differ chip | `manipulateSavefile` ops, `setSavefilePlayer1`, `renameSavefile` | M |
| 6 | Servers: internet list, mod check, direct join, history | `steamRequestInternetServersList`, `steamGetInternetServerDetails`, `serverConnect(...)`, `Server:getMods()` | L |
| 7 | Hosted server settings: apply your mods to your server | `getServerSettingsManager()`, `getServerOptions():getOptionByName("Mods"/"WorkshopItems"/"Map")`, `duplicateFiles` | M |
| 8 | Details: updated date, version, category, Needed by names, Incompatible list, scroll, jump to a requirement | `getModVersion`, `getCategory`, `incompatibleWith` | S |
| 9 | Filters (Favourites, category), sort, author search, undo, multi-select and bulk on/off, Hide | model fields, plus a file for Hide | M |
| 10 | Workshop: look up an item or collection by ID, search in the Steam overlay, download size | `querySteamWorkshopItemDetails`, `getFileSize`, a Workshop URL with `searchtext=`, `getSteamWorkshopItemMods` | M |
| 11 | File overlaps, partly: same-named Lua and script files | `listFilesInModDirectory` (folders aren't listed) | M |
| 12 | Make a mod, partly: edit and publish updates to your own mods | `getSteamWorkshopStagedItems()`, `getModFileWriter` | M |
| 13 | Lists and mods with Java in `42/` or `common/` | `listFilesInModDirectory` / `getModFileReader` per build | S |

Still impossible in game:
- Save backups and restore (no copy).
- Change notes and news (no internet).
- Subscribe and install.
- A new mod folder.
- ZombieBuddy setup.
- Launch options.
- Server hosting tools.

## Phase 3: what the app unlocks, with both installed (game side built 2026-10-15; app side in the Desktop App session)

1. **Profiles** (M): a profiles file; switch profile from the game, and the app confirms.
2. **Check setup results in game** (S): a findings file, shown under Problems, with Fix buttons that send requests.
3. **Backups** (S): a backups file per save; Restore through a request; "Back up first" on Delete.
4. **Workshop pictures and titles** (S, unverified): PNGs in `Lua/pzmm/img/`. Test first that `getTexture` loads a full path.
5. **Updates** (S): use the feed's `pending` flag, with an Update button that sends an install request. UPDATES counts pending updates, as the app does.
6. **What's new** (S):
   - The app writes `feed.txt` on every feed load, Home included, with an `at=` time.
   - The game drops entries older than your last play.
   - The seen state (`feedseen.txt`, `played.txt`) is shared with the app.
7. **Settings** (S): the logo and watermark settings are shared both ways.
8. **Ideas:**
   - "Why isn't it loading?" for one mod.
   - The last session's mod errors shown on the next launch.
   - A save that remembers its profile.
   - Roll back a mod's last update.
   - Share codes.

## Phase 4: layout, language and input (built 2026-10-15 except: translations cover the tabs and toolbar so far; a controller still opens the game's own screen)

1. **Layout** (M):
   - Home: Continue, latest save and stats, plus one short line on the app. No sorry card.
   - The app pitch lives only in PZMM Settings, under About, and can be dismissed.
   - Per-tab notes are one line, shown only where a control is missing.
   - Order: Home, My mods (Check setup there), Saves, Maps, Servers, What's new, Java mods, Browse Workshop. Make a mod moves under Settings.
2. **Wording** (S): one word per idea. Profile for the whole set, Sections for its groups, Accept for the game's save, Apply for the app's.
3. **Keyboard and controller** (M-L): Esc for Back, arrows, Enter, and the joypad A, B and shoulder buttons. Until then, fall back to the game's own screen on a controller.
4. **Translations** (L, a little at a time): `Translate/<LANG>/UI_<LANG>.txt`, generated from the app's 10 locales, used through `getText` with `%1`.
5. **Text size** (M): read font heights again on resize, and add a "Larger text" setting.
