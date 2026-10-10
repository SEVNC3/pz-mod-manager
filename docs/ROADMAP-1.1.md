# PZMM 1.1: review findings and scope

2026-10-10. **Status: queue worked through for 1.1.0 (unreleased); see "What shipped" at the end.** Merged from three read-only reviews (UX/product, code robustness, community research) plus the Project Viewpoint scope. Effort: S = hours, M = a day or two, L = more.

## P0: data safety (fix before the next release)

All verified in the code. None have been reported as hitting a user yet.

| # | Problem | Failure | Fix | Effort |
|---|---|---|---|---|
| 1 | Mod id used as a folder name | A mod.info with `id=..` makes install, remove or rollback target `Zomboid/` itself, which gets moved or deleted (`store.rs` `folder_name`) | Reject empty, `.`, `..` and Windows-reserved names. Assert the destination's parent is `mods/` | S |
| 2 | Installing over a mod PZMM doesn't own | A hand-installed `mods/Foo` is moved into the backup slot; the next install deletes it. Updating twice with identical content also erases the real previous version | Never replace unindexed folders without asking. Skip backup rotation when content is unchanged | S |
| 3 | Unreadable or missing `default.txt` | Read error (game has it locked, bad encoding) means "nothing enabled", so sync switches off the whole profile | Return `Option` from `read_default` and skip adopting when it's unreadable | S |
| 4 | Non-atomic writes, `.bak` overwritten each time | A crash mid-write corrupts `installed.json` and every command then fails. After two saves the original `default.txt` / `.ini` backup is gone | One `write_atomic` (tmp + rename). Keep the first backup | S |
| 5 | Unvalidated Workshop ids | `WorkshopItems=` from a server `.ini` flows into SteamCMD args and paths (`+runscript`, `../..`) | Digits only in `Source::Workshop` and `download()` | S |
| 6 | ZombieBuddy jar source | Any mod with id `ZombieBuddy` (e.g. a local zip) supplies the JVM agent copied into the game folder | Only take it from Workshop item 3619862853 | S |
| 7 | Duplicate ids | Server import or save import with duplicate ids leaves an empty `+` line and `mod = \,` in `default.txt`. On Windows, `profile new default` overwrites `Default` | Dedupe in parse/adopt, drop empty ids, case-insensitive profile names on Windows | S |
| 8 | Half-finished installs | A copy error mid-install leaves a half-copied mod and unindexed siblings | Copy to `dest.tmp`, rename, save the index per mod | M |

## P1: robustness and speed

- **Downloads freeze the app.** The app holds its global lock for the whole SteamCMD download, so every other command waits, and SteamCMD has no timeout. The CLI takes no lock at all. Download outside the lock, add a timeout, and use a lock file shared with the CLI. (M)
- **Slow with 300+ mods.** `scan` runs on every command, including each row click. Cache it and invalidate on writes, use `DirEntry::file_type()` instead of a stat per entry, and cache per-mod file sets for conflicts. (M)
- **Steam rate limits.** Throttle page requests (about 300 ms), give `post_form` the same 429 retry, and cache required-items lookups. (S)
- **mod.info quirks.** A UTF-8 BOM hides `id`, so the mod gets the wrong id. Strip it. (S)
- **`maps()`.** It only checks `42/` and `common/`. Use any version folder and dedupe `Map=`. (S)
- **Test gaps.** Add tests for hostile ids, install failure, BOM/CRLF, `unzip`, and duplicate ids. The release workflow also skips the app crate's tests. (S)

## P2: features, ranked by value

| # | Feature | Why | Source | Effort |
|---|---|---|---|---|
| 1 | Undo (Ctrl+Z / toast) for list changes | One misdrag in a 150-mod list rewrites `default.txt` with no way back | UX | S–M |
| 2 | Honour all mod.info keys: `incompatible`, `loadModAfter`, `loadModBefore`, `versionMin`, `versionMax`, plus an auto-sort | Load order is the #1 community pain. The engine reads these keys (semantics to verify in game) | Research | M |
| 3 | "Doctor" preflight: B41-only mods on B42, unknown ids, delisted items, map order, server `Mods=` vs `WorkshopItems=` drift. Also a CLI exit code | Silent skips and "workshop item version is different" kicks | Research | S–M |
| 4 | Save-safe changes: warn when a change differs from the active save's mods; snapshot before updates | Reordering or updating mid-save loses saves | Research | S |
| 5 | Bulk select (shift/ctrl-click) with enable, disable, move and remove | Changing 20 mods is 20 clicks | UX | M |
| 6 | Export or import a profile as a shareable file or code | Friends and servers can't share lists | UX + research | M |
| 7 | `console.txt` error attribution plus mod bisect | Lua error walls with no culprit | Research | M |
| 8 | Collapsible separators, add a separator at the selection, "Move to…" | 100+ mod lists | UX | M |
| 9 | Columns and sorting: size, last updated, status | Only drag ordering today | UX | M |
| 10 | Server admin: launch the dedicated server, a headless update watcher, restart warnings | Server pulls updates only at boot | Research + UX | M |
| 11 | Compatibility badges: B41-only, not updated since the last game patch | B42 breakage | Research | S–M |
| 12 | Accessibility: Enter selects a row, `/` focuses search, arrow keys on tabs and filters, wider status column | Keyboard users | UX | S |

**Quick wins (S):** fix the empty-state flash on first load, keep the keyboard hint visible, show unresolved missing mods per row, default the server name to an existing config, label "Update all" as re-downloading.

**Not worth it now:** a Lua malware scan (no user demand found).

## Project Viewpoint mode

**What it is.** [Project Viewpoint](https://steamcommunity.com/sharedfiles/filedetails/?id=3809306528) (Workshop 3809306528, mod id `Viewpoint`, about 354k subscribers, B42 only, early development) replaces the isometric renderer with a 3D first/third-person engine and runs multiplayer. Its only listed requirement is [ZombieBuddy](https://steamcommunity.com/sharedfiles/filedetails/?id=3619862853). It has a growing add-on scene (3D model pack, Door Fix, Surface Fix, QOL, Parkour, Recoil…).

**The feature.** A **Project Zomboid / Project Viewpoint** switch in the header. Each side remembers its own profile, so you flip between your 2D modlist and your 3D modlist in one click.

1. **First switch to Viewpoint.** One preview dialog listing every change, then:
   - Download ZombieBuddy and Viewpoint if they aren't subscribed.
   - Set up the ZombieBuddy Java agent (the existing ZombieBuddy page flow).
   - Create a "Viewpoint" profile from the current one, with ZombieBuddy and Viewpoint enabled at the top.
   - Switch to it.
2. **Switching back.** Activates the last Zomboid profile. Viewpoint stays installed but disabled. The ZombieBuddy agent stays, because other Java mods may need it, and the ZombieBuddy tab can remove it.
3. **Viewpoint add-ons.** In Viewpoint mode the Workshop tab offers a "Viewpoint add-ons" shortcut, a search for "Project Viewpoint".
4. **Guards:**
   - Only on B42.
   - If either required mod is missing, the switch shows "Needs setup" instead of failing.
   - Before the first launch, warn that ZombieBuddy will ask once to approve Viewpoint's Java code.

**Data model.**
- A small built-in table of modes: `id`, `name`, `workshop: [3619862853, 3809306528]`, `mods: [ZombieBuddy, Viewpoint]`, `java_agent: true`.
- A `mode` field per profile in `profile.json`, and the active mode in `state.json`.
- No new formats. The table could later hold other total conversions.

**Open questions:**
- **Client-only?** Viewpoint says you can join "about any online server … as long as its modlist is compatible", which suggests it's client-side. If so, server export should leave Viewpoint (and ZombieBuddy?) out of `Mods=`. Needs confirming with its author or Discord.
- **Updates.** Viewpoint hotfixes often, and its page suggests re-subscribing on issues. Prefer the Steam subscription over a PZMM copy so Steam keeps it current. Installs also need a fix: today a PZMM copy in `Zomboid/mods` can shadow the newer subscription.
- **ZombieBuddy policy.** Some 3D mods' guides tell users to set `policy=allow-all`. PZMM keeps ZombieBuddy's default "prompt" policy and never sets allow-all on its own.

**Effort:** M (about 1–2 days): the core mode table and profile field, the header switch, the first-run setup dialog reusing the ZombieBuddy and install code, the subscription preference, and server-export filtering once client-only is confirmed.

## What shipped (1.1.0, unreleased)

**Done:**
- Every P0 item.
- **P1:** downloads outside the lock, SteamCMD limits, the cross-process lock, Steam throttling and caching, the scan cache, `file_type()`, the BOM and `Mod.info` fixes, maps in any version folder, and the tests.
- **Viewpoint mode:**
  - the header switch with a confirmation step
  - `pzmm mode`
  - each mode remembering its own profile
  - Steam subscriptions preferred over PZMM copies
- **P2:**
  1. Undo and redo.
  2. `incompatible` and `loadModAfter`/`loadModBefore`, with the stable sort.
  3. Doctor and `pzmm doctor`.
  4. The save-mismatch banner and the confirmation before updating a save's mods.
  5. Bulk select.
  6. Profile share and import (code, plus `pzmm profile export`/`import`).
  7. `console.txt` error attribution, `pzmm errors` and the bisect helper.
  8. Collapsible separators, separator at the selection, and Move to.
  9. Size and last-changed columns, with sorting.
  10. Start server, and `pzmm watch` with `--on-update`.
  11. B41/B42-only badges.
  12. Keyboard: Enter, `/`, arrow keys on tabs and filters, and a shortcut list.
- **Quick wins:** the loading state, the server-name default, and the status column width. The empty-state "flash" turned out not to exist: before the first load the list is empty, not the "nothing yet" message.

**Added after this review (also in 1.1.0):**
- **Autosort and categories** (`pz/category.rs`): sections from Workshop tags, every load rule satisfied, a Load order panel beside the list, new profiles sorted on first run.
- **Section dragging**, removable separators, and loose mods (removing a separator leaves its mods in place).
- **Workshop parity** with Steam's anonymous Workshop: see [WORKSHOP-PARITY.md](WORKSHOP-PARITY.md).
- **Installs** enable only what's needed and compatible.
- **Look:** a frameless window with its own title-bar buttons, the blood-red theme and logo, blueprint backgrounds with shadowed cards, and one profile menu grouped by game. See [UI-QUEUE.md](UI-QUEUE.md) and the design system.
- **Steam client:** Workshop pages open in the Steam client when it's running.
- **Links:** GitHub, Discord and Ko-fi in the tab bar.
- **The pre-release queue** in [RELEASE-1.1-QUEUE.md](RELEASE-1.1-QUEUE.md): data-loss, security and freeze fixes found by a second round of reviews.

**Deliberately skipped (each with a reason):**
- **Copying the save folder before updates.** Saves can be gigabytes. Mod rollback plus the save warning cover the risk.
- **A "not updated since the last game patch" badge.** There's no reliable patch date, and it would flag many stable mods.
- **RCON restart warnings.** `pzmm watch --on-update` runs the admin's own command, which can use their RCON tool.
- **The `versionMin`/`versionMax` check.** It needs the installed game version, which PZMM can't read reliably yet. `versionMin` is shown in the details pane.

**Still open from the plan:**
- Excluding Viewpoint from server `Mods=`, if it's confirmed client-only.
- GitHub update checks.
- The `default.txt` `maps {}` block.
