# Pre-1.1.0 queue

Five reviews ran on 2026-10-10:
- **core:** bugs in the Rust core and CLI
- **app:** the Tauri commands, the Svelte code and refactors
- **ui:** visual consistency
- **ux:** flows
- **hygiene:** docs and release readiness

This merges their findings. Each item names its source. Work top to bottom; P0 and P1 block the release.

## P0: data loss, security, freezes

1. [ ] **Server export wipes the .ini when it can't read it** (core 1). `server.rs:57`: start empty only on NotFound; any other read error stops the export.
2. [ ] **An unreadable modlist.txt gets replaced by the game's list** (core 2). `profile.rs:257`: fall back only when the file is missing. Same for state.json (core 20) and default.txt (core 12).
3. [ ] **A save silently drops changes made in the game's mod menu** (app 1).
   - `save` takes `adopted` from `sync`; when the game's list changed, reload and say so instead of overwriting.
   - Reload when the window regains focus.
4. [ ] **Starting a server passes its name to cmd.exe unescaped** (core 5, app 9). Run the .bat directly with CREATE_NEW_CONSOLE, and reject `& ^ % ( ) ! , ;` in server names.
5. [ ] **Updating a mod while the game has it open half-deletes it, then poisons the backup** (core 3). `move_dir` copies only across devices; otherwise it returns the rename error with "close the game and try again".
6. [ ] **ZombieBuddy files are overwritten in place** (core 11). Use `write_atomic`, so a jar in use triggers the `.new` fallback.
7. [ ] **The window freezes on row clicks and after every toggle** (app 2).
   - Make every sync command `#[tauri::command(async)]`.
   - Walk files for `conflicts` and `conflict_summary` outside the lock.
8. [ ] **`busy` resets early, so a second install can start mid-install** (app 3). Count pending runs; row keys, drops and renames ignore input while busy.
9. [ ] **`open_path` opens anything; `opener:default` bypasses the URL allow-list** (app 10). Folders only; drop the permission and the unused npm package.

## P1: correctness and must-fix UX

10. [ ] **Profile names are case-sensitive but the folders aren't, so the active profile can be deleted** (core 4).
11. [ ] **Deleting a profile can switch you into the other game mode** (app 4). Pick the next profile from `modeProfiles`.
12. [ ] **Mod ids from non-English Workshop descriptions panic or are missed** (core 6). Use `to_ascii_lowercase`.
13. [ ] **A failed SteamCMD download is reported as success from stale content** (core 7). Require "Success. Downloaded item <id>".
14. [ ] **One bad Workshop id aborts the whole batch** (core 9). Drop invalid ids with a per-id error.
15. [ ] **The `search` and `watch` CLI commands sync the profile without the lock** (core 8).
16. [ ] **The Workshop tab resets every time you leave it** (ux 1, app 8). Keep it mounted after the first visit (`hidden`, plus a global `[hidden]` rule).
17. [ ] **"Update all" re-downloads everything with no confirm** (ux 2).
    - Rename it "Re-download all", confirm with a count, and run the latest-save check.
    - Add "Check for updates".
18. [ ] **Autosort copy says it keeps your order, but it replaces your separators** (ux 3).
    - Say so, and confirm when there are custom separators.
    - If Workshop tags can't be read, fall back to fixing load rules only (ux 4).
19. [ ] **Undo after Find the broken mod restores the half-disabled list** (app 5). Clear undo on bisect start and stop.
20. [ ] **Separators with the same name share collapse, counts and Move to** (app 6). Give new separators unique names; renaming refuses a duplicate.
21. [ ] **Autosort can hang on cyclic load rules** (app 7). Look rows up through an index map, and cap the passes.
22. [ ] **Workshop item panel** (ux 5, app 14).
    - Add a dialog role, focus handling and a focus trap.
    - Show change notes and comments only for the current item.
    - Give each tab its own error.
23. [ ] **Light theme: white text on the pale blood-red highlight is unreadable** (ui 1). Add `--on-selected` (white on Charcoal, ink on Daylight) and use it everywhere `#fff` sits on hover or selected states. The blueprint lines also follow the theme.
24. [ ] **"Update and restart" while busy kills the install** (app 13, ux 21). Disable it while busy, and add a Later button.

## P2: consistency and polish

25. [ ] **One selected state** (ui 4-8).
    - Add a `--sel` token used by tabs, segmented controls, rows and the item panel's tabs.
    - Hover is lighter than selected, not identical.
    - Hover and selected text is always `--on-selected`, including disabled rows.
    - A required tag chip uses the selected style.
26. [ ] **Servers & saves and ZombieBuddy on the blueprint with cards** (ui 3). Centre them.
27. [ ] **Banners, the mode plan, import, findings, settings and the app-update strip as one card style** (ui 18). Lay out Settings as a grid (ui 19).
28. [ ] **Icons** (ui 11, 12, 14, 15, 25).
    - Missing icons: Details Disable, the bulk bar, Fix N problems, the banners, ZombieBuddy apply, Write .ini, Import.
    - Chevron icons on pagers; grip and chevron icons instead of `⠿` and `▸▾`; Feather checks in ZombieBuddy.
    - Sizes: 14 in buttons, 16 icon-only, 12 inline.
29. [ ] **Headings** (ui 16). A global h2 (20) and h3 (12 uppercase label); remove the per-file copies.
30. [ ] **Native controls** (ui 2, 21).
    - Checkbox `accent-color` everywhere.
    - Date inputs like the other inputs.
    - Replace the datalist with the styled select.
31. [ ] **Progress bar and idle activity dot are invisible** (ui 17, 25).
32. [ ] **The bulk bar wraps the toolbar; show it only for 2+ picked rows** (ui 9, ux 9). Hide the hint on narrow windows (ui 10).
33. [ ] **Workshop count, pager and messages sit on a panel strip, not on bare blueprint** (ui 13). Add a loading state and per-row retry on Home; load the rows in parallel (ux 13). Map 429 to plain words, and add Clear filters to the empty state (ux 14).
34. [ ] **Two filled buttons in the bisect panel** (ui 24). Tiles get borders (ui 22).
35. [ ] **Plain `http://` links in descriptions** (app 11). Rewrite them to https.
36. [ ] **Collapsed sections swallow moved rows** (app 12, ux 20). Step over hidden rows; dropping onto a separator puts the row under it.
37. [ ] **Unhandled rejections** (app 15): planMode after Esc, Details loads, openUrl failures go to the activity log, and "Update all" clears updates only on success.

## P3: refactor (after P0-P2, behaviour unchanged)

38. [ ] **Shared helpers** (app B2, B3).
    - `lib/format.ts`: size, date, compact numbers, workshopUrl, plain.
    - `Stars.svelte` and `InstallButton.svelte`.
39. [ ] **Shared CSS in app.css** (app B4): segmented control, `button.link`, `.hint`, `.lede`, `.error`, `.pager`. Merge the split rules in +page.
40. [ ] **One install path and one `run(label, job, opts)`** (app B1, B5).
41. [ ] **Split +page.svelte** (app B6-B10). ActivityLog, SettingsPanel and Findings, then ModTable; later an app state module.
42. [ ] **Rust tidy** (app B11).
    - One `blocking` helper.
    - `reading` for read-only probes.
    - A cached media-file walk.
    - `update_all` under the lock.
43. [ ] **Names and slim saves** (app B12). Rename the `ModDetails` type; save sends `{kind,id,on}` only.
44. [ ] **Dead code** (app C): the `enable` parameter, the `copy` icon, `SepRow`, and the redundant lines in Details.

## Lower-priority core fixes (1.1.x)

- Updates continue for mods shadowed by a later Steam subscription (core 10).
- B41 format kept for an empty list (core 13).
- Char-boundary slice in the required-items scraper (core 14).
- `find_mods` stays inside its root (core 15).
- Rollback and remove handle stale backups (core 16).
- `server import` doesn't overwrite an existing profile (core 17).
- fsync in `write_atomic` (core 18).
- Wait for and kill SteamCMD's process group on timeout (core 19).

## Docs and release (hygiene)

- [ ] `package-lock.json` version 1.1.0; the release workflow checks package.json against the tag.
- [ ] Node: say "22.18 or newer", or pin it.
- [ ] README:
  - Workshop features.
  - Autosort and section dragging.
  - The "Separator" label.
  - CLI examples.
  - A Credits section (Feather, the fonts, logo.dev logos, game trademarks, not affiliated).
  - CLI help lists PZ_GAME_DIR and PZ_SERVER_DIR.
- [ ] ROADMAP-1.1 "What shipped" brought up to date. FOUNDATION.md stale files (`reconcile.rs`, `local.rs`, 7z) and the 1.1 row fixed. Add WorkshopPanel to WORKSHOP-PARITY. Mark UI-QUEUE as superseded where later commits changed things.
- [ ] Remove the unused `serde_json` (app crate) and `@tauri-apps/plugin-opener` (npm).
- [ ] Release notes from the hygiene draft (`--notes-file`).
- [ ] Decide before pushing:
  - Squash the GIF add and remove commits (840 KB in history).
  - Use the GitHub noreply email.
  - Fill in or hide the Discord and Ko-fi links.

## Ideas for after 1.1 (ux 25)

- "What changed" for updates.
- List snapshots before updates and autosort.
- Rename and duplicate profiles; paste a share code from the clipboard.
- A right-click row menu.
- A Browse… button for zips.
- "Hide installed" in the Workshop.
- Cancel and n-of-m progress for installs (ux 12).
- Back in the item panel and after "Mods that need this" (ux 6, 7).
- An install action for missing mods; "Disable X" on incompatibilities (ux 10, 11).
- Previews before writing a server .ini or switching to a save's mods (ux 16, 17).
- A copyable activity log with times (ux 22).
- Launch game warns about problems (ux 24).
