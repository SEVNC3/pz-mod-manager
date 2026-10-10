# Foundation Plan: PZMM (Project Zomboid Mod Manager)

Status: v1.0 built; 1.1 in [ROADMAP-1.1.md](ROADMAP-1.1.md) · 2026-10-10

## 1. Scope

**A Project Zomboid mod manager for players and server admins, on Windows and Linux.** It's built so another game could be added later, but nothing is built for that now.

Not in scope: other games, hosting mods, a website, Nexus Mods, a virtual file system, plugins, community themes and translations. Each is revisited only when there's a real need.

**Portability rule:** all Project Zomboid knowledge lives in one module (`pz`). Everything else (mod store, profiles, downloads) knows nothing about PZ. A second game means writing its own module first, then extracting the shared interface from the two real examples. We don't design that interface up front.

## 2. What we take from MO2 and Relapse's Mod Manager Redux

Only what serves Project Zomboid:

| From | Idea | In PZ terms |
|---|---|---|
| MO2 | Profiles as a plain-text, ordered list (`+Mod` / `-Mod`) | Mod setups you can switch between: "Solo B42", "Friends server", "Testing". Line order is the load order. |
| MO2 | Conflict view (winning/losing files) | Files under `media/` (`lua/`, `scripts/`, textures) that several mods provide, and which mod wins by load order. |
| MO2 | Separators | Group a 100-mod list into sections ("Maps", "QoL", "Vehicles"). |
| MO2 | Launch list | Launch the game and the dedicated server from the app. |
| RMMR | Reconcile on start | Compare `Zomboid/mods` and `default.txt` with our records, and show a review before any repair. |
| RMMR | File ownership | Only touch folders we installed. Workshop subscriptions and hand-installed mods are read-only unless you say otherwise. |
| RMMR | Backup before update, "Restore previous version" | One-click rollback when a Workshop update breaks your save. |
| RMMR | Presets with a protected base | "Return to base" for a profile, and a prompt before discarding unsaved changes. |
| RMMR | Update status per mod, thumbnail, source link | Workshop preview image, last-updated date, an "update available" badge. |
| RMMR | Dark/light that follows the OS, plus an accent colour | Built-in themes only. |
| RMMR | Single-instance lock, copy-diagnostics button with paths redacted | Same. |

**Avoid:**
- Injecting into the game process (MO2's usvfs).
- Renaming folders to disable mods (RMMR).
- Absolute paths in saved data.
- Logs that grow forever.
- A separate second load order.
- A long tutorial.

## 3. Architecture

**Stack: Tauri 2, with a Rust core and a Svelte + TypeScript interface.**
- The download is small (about 10 MB) and the app is native on Windows and Linux.
- It gets fewer antivirus false alarms than packaged Python apps.
- A single-file `pzmm` command-line tool shares the same core, so server admins on Linux have nothing to install.

```
crates/core/
  pz/            ALL Project Zomboid knowledge: paths, mod.info, B41/B42 layout,
                 default.txt, server .ini, saves, categories, doctor, logs,
                 files (conflicts, maps), Viewpoint, ZombieBuddy
  store.rs       installed mods, the install index, ownership, backups
  profile.rs     modlist.txt (+/- lines, separators), state.json, sharing
  sources/       mod.rs (fetch, install, local zip or folder), steam_workshop.rs, github.rs
crates/cli/      `pzmm` binary: same commands as v0.1
app/             Tauri shell + Svelte UI
```

**On disk:**
- **Mods** go where the game already loads them: `Zomboid/mods/<ModId>/`. Mod folders are left untouched; what PZMM owns is recorded in `<data>/installed.json`.
- **The app's own data** lives in a per-user folder (`%LOCALAPPDATA%\pzmm` on Windows, `~/.local/share/pzmm` on Linux):

```
<data>/
  installed.json                mod id -> source (ours to update/remove)
  state.json                    active profile, forced build format
  profiles/<name>/modlist.txt   "+ModA" "-ModB" "# --- Maps"
  profiles/<name>/profile.json  build (41/42), linked server .ini
  backups/                      previous mod versions + default.txt snapshots
  steamcmd/                     downloaded on first Workshop install
  logs/                         rotating
```

All paths are stored relative to `Zomboid/` or `<data>/`. Applying a profile only rewrites `default.txt` (and, if linked, the server `.ini`). Files are never moved.

## 4. Features for v1

| Feature | Notes |
|---|---|
| **Install** | Workshop item or **collection** URL (SteamCMD anonymous login, collection lookup without an API key). GitHub repo (release asset first, then zipball). Local zip or folder. |
| **See everything** | Mods in `Zomboid/mods` and Steam Workshop subscriptions together. Each row shows its source and a status icon. |
| **Profiles** | Switch mod setups. Drag to set load order. Separators. Return to base. |
| **Build 41 / 42** | Detect the layout (`42/`, `42.x/`, `common/`) and `versionMin`, write the correct `default.txt` format, and flag mods meant only for the other build. |
| **Dependencies** | `require=` in `mod.info`. Warn when a dependency is missing, disabled or loads after the mod that needs it, and offer to install it. |
| **Updates** | Workshop last-updated time (looked up without an API key), GitHub tag or commit. "Update all" makes a backup first, with per-mod rollback. |
| **Server** | Profile → `Server/<name>.ini` (`Mods=`, `WorkshopItems=`, `Map=`). The reverse too: import a server's `.ini` as a profile ("join my friend's server"). |
| **Saves** | Read each save's `mods.txt`, compare it with the active profile, and offer "load this save's mods". |
| **Maps** | Detect `media/maps/*` and manage the `maps { }` block and `Map=`. |
| **Conflicts** | Files overridden between mods, and duplicate mod IDs (Workshop vs local copy). |
| **Launch** | Game via `steam://rungameid/108600`, and the dedicated server script. |

**Check in the game before building:** the exact Build 42 `default.txt` format, whether B42 `mod.info` has load-order or incompatibility fields, and per-save `mods.txt` on B42.

## 5. UI

```
┌ Profile ▾ │ Install… │ Update all │ ▶ Launch ▾ │ ⚙ ──────────────────────────┐
├ search · [Enabled] [Updates] [Problems] [Workshop|GitHub|Local] ─────────────┤
│ ☑ # Name            Ver   Source     ⚑ │ thumbnail · description · link       │
│ ── Maps ──                             │ Info · Files · Conflicts · Requires  │
│ ☑ 1 ...                                │                                      │
├────────────────────────────────────────┴──────────────────────────────────────┤
│ activity / downloads (collapsible)                                             │
└────────────────────────────────────────────────────────────────────────────────┘
```

- **One status icon per row** (⚑): an update is available, a dependency is missing, there's a conflict, or the mod is for the other build.
- **First run:** find `Zomboid/` and Steam, then import what's currently enabled as the "Default" profile.
- **Accessibility:** everything works from the keyboard, colour is never the only signal, and the UI follows the system font size.

## 6. Roadmap

| Version | Content | Done when |
|---|---|---|
| **0.2 · Core** | Cargo workspace, a port of the v0.1 logic into `core` (`pz/`, store, sources), profiles and the `pzmm` command line with v0.1's commands. CI on Windows and Linux. | `cargo test` passes on both systems, and existing v0.1 installs are picked up unchanged. |
| **0.3 · App** | Tauri + Svelte: mod table, details pane, install, enable/disable, drag to reorder, profiles, themes. Windows installer and Linux AppImage. | Covers everything v0.1 does in the new interface. |
| **1.1 · Load order and Workshop** | Autosort, section dragging, full Workshop parity, Viewpoint mode, server start and watch, sharing, doctor, bisect, the new look. See [ROADMAP-1.1.md](ROADMAP-1.1.md). | A player can sort a 100-mod list and browse the Workshop as fully as on Steam, without leaving PZMM. |
| **1.0 · PZ, done well** | Collections, updates with rollback, dependencies, server export and import, saves, maps, conflicts, launch. | A B42 player can go from a fresh install to 50 mods to a running server without opening a file. |

**v1.0 status:**
- **Done:** everything above, plus two additions: a Workshop browser tab and ZombieBuddy setup (Java agent files plus the launcher flag).
- **Not done:**
  - Update checks for GitHub sources.
  - Managing the `maps { }` block in `default.txt`. Servers already get `Map=`.
- **Still unverified in a real B42 game:** the `default.txt` format.

**Later, only if you move into another game:** add `crates/core/<game>/` next to `pz/`, then extract a `Game` trait from the two real modules. The store, profiles and sources are already game-neutral.
