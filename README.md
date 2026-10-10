# PZMM: Project Zomboid Mod Manager

A free, open-source mod manager for **Project Zomboid** (Build 41 and 42), for Windows and Linux.

- **Browse the Workshop** inside the app, with everything Steam's own Workshop offers without logging in:
  - search titles or descriptions, and sort by popularity over any period from today to a year
  - require or exclude any of Project Zomboid's tags, and filter by build, posting or update date
  - browse collections and install a whole one at once
  - a Home view with this week's popular mods, the most subscribed, the newest and the libraries other mods need
  - an item panel with screenshots, videos, the full description, change notes and comments, plus the mods that need it, the collections it's in and more by the same author
- **Install** from the Steam Workshop (link, ID or a whole collection), from GitHub (`owner/repo`, optionally `@tag`) or from a local `.zip` or folder. You don't need to log in to Steam. If a mod lists other mods it requires, PZMM installs those too.
- **See everything:** Steam Workshop subscriptions and the mods in your `Zomboid/mods` folder, in one list with each mod's poster, author and description.
- **Library and load order**, kept apart like Mod Organizer 2:
  - the library on the left is yours to organise: separators, dragging whole sections, **Group by category** from Workshop tags, and switching mods on and off
  - the load order beside it is what the game loads: drag it, or **Autosort** it (frameworks first, translations last, every `require` / `loadModAfter` / `loadModBefore` rule satisfied)
  - PZMM writes the same `mods/default.txt` the game reads, and a map order for dedicated servers
- **Settings:** every folder PZMM uses with Browse and Detect, update checks, Recycle Bin removal, backup limits and launch arguments.
- **Profiles:** keep separate mod setups ("Solo", "Friends server") and switch between them from one menu that groups them by game. Changes you make in the game's own mod menu are picked up automatically, and PZMM never saves over them.
- **Dependencies:** PZMM flags enabled mods whose required mods are missing, disabled or loading too late. **Install missing mods** finds the missing ones on the Workshop: it checks the "Required items" the author lists, and reads "Mod ID: X" from Workshop descriptions. **Fix load order** sorts the rest.
- **Conflicts:** each mod's details show which other mods replace the same files, and which copy wins.
- **Updates:** PZMM checks the Workshop for newer versions of the mods it installed when it starts, and whenever you click **Check for updates**. Every update keeps the previous version, and **Restore previous version** brings it back if an update breaks your save.
- **Servers and saves:** write a profile into a dedicated server's config (`Mods=`, `WorkshopItems=`, `Map=`), turn a server config into a profile, or load the mods a save was played with.
- **Project Viewpoint:** switch between plain Project Zomboid and [Project Viewpoint](https://steamcommunity.com/sharedfiles/filedetails/?id=3809306528) (3D first/third person) from the header. The first switch downloads Viewpoint and ZombieBuddy, sets up ZombieBuddy and makes a Viewpoint profile; each side remembers its own mod list.
- **ZombieBuddy:** one-click setup for [ZombieBuddy](https://github.com/zed-0xff/ZombieBuddy), the loader that Java mods need. PZMM shows each change before applying it and can undo it.
- **Updates itself:** PZMM checks for new versions when it starts. **Update and restart** downloads the new version, checks its signature and installs it.
- **Find what's wrong:** **Check setup** reports:
  - mods built for the wrong game build
  - missing, disabled or late-loading requirements
  - incompatible mods
  - server configs out of sync

  It also counts the Lua errors in your last session's `console.txt` per mod. **Find the broken mod** narrows down a crash by halving your mods each time you test the game.
- **Big lists:** select several mods at once (Ctrl/Shift+click), fold separators, sort by name, size or last change, and undo with Ctrl+Z.
- **Share a profile:** **Share** copies a code. A friend pastes it into **Import**, and PZMM installs the Workshop mods they're missing.
- **Saves:** PZMM warns when your latest save was played with a different mod list, and offers to load the save's list.
- **Ownership:** PZMM only deletes mods it installed. It never touches your Workshop subscriptions or mods you installed by hand.

## Download

From the [latest release](../../releases/latest):

| System | File |
|---|---|
| Windows | `PZMM_<version>_x64-setup.exe` |
| Linux | `PZMM_<version>_amd64.AppImage` (any distribution) or `.deb` (Debian/Ubuntu) |
| Servers (no desktop) | `pzmm-linux-x64` or `pzmm-windows-x64.exe`: the command-line version, a single file |

The Windows installer isn't code-signed yet, so Windows may show a SmartScreen warning. Click **More info → Run anyway**.

## Use

1. Paste a Workshop link, a GitHub repo or a zip path into the box at the top, then click **Install**. The mod is installed and enabled.
2. Tick or untick mods to enable or disable them. Drag rows to change the load order. With a row selected, you can also use the keyboard: **Space** toggles it and **Alt+↑/↓** moves it.
3. Use **Separator** to group your library, or **Group by category** to sort it into sections, and **Autosort** to fix the load order. Double-click a separator to rename it, drag it to move its whole section, and press **Delete** to remove it (its mods stay where they are).
4. Click **Launch game**.

**Build 41 or 42?** PZMM keeps whatever format your `default.txt` already uses. If the file is new, PZMM uses the Build 42 format. To set it yourself, go to **Settings → Game build**. Before each save, the previous file is kept as `default.txt.bak`.

## Command line (`pzmm`)

```
pzmm add 2169435993 someone/their-pz-mod ./MyMod.zip
pzmm list
pzmm enable ModId
pzmm disable ModId
pzmm remove ModId
pzmm update
pzmm rollback ModId          # restore the version before the last update
pzmm add --no-enable 2169435993   # install without enabling
pzmm profile list
pzmm profile new Server
pzmm profile use Server
pzmm profile delete Server
pzmm build 41                # force the B41 default.txt format
pzmm search snow --sort toprated --build 42 --page 2
pzmm outdated                # Workshop mods with updates waiting
pzmm find tsarslib --install # find the Workshop item that provides a mod id
pzmm server export servertest   # active profile -> Zomboid/Server/servertest.ini
pzmm server import servertest   # server config -> profile (installs missing mods)
pzmm zombiebuddy install     # also: status, uninstall
pzmm mode viewpoint          # or: zomboid; --dry-run lists the changes
pzmm doctor                  # check the profile and server configs (exit code 1 on errors)
pzmm errors                  # Lua errors per mod in the last game session
pzmm profile export friends.json   # share a profile; import with: pzmm profile import friends.json
pzmm server start servertest # start the dedicated server
pzmm watch servertest --every 10 --on-update "restart-server.bat"   # check every 10 minutes; run a script on updates
```

The command line and the app share the same profiles and data.

On Linux, SteamCMD needs 32-bit libraries (`lib32gcc-s1` on Debian/Ubuntu).

| Variable | Default | Purpose |
|---|---|---|
| `PZ_HOME` | `~/Zomboid` | Your Zomboid user folder |
| `PZMM_DATA` | `%LOCALAPPDATA%\pzmm` or `~/.local/share/pzmm` | Profiles, backups, SteamCMD |
| `PZ_GAME_DIR` | found in your Steam libraries | The game's install folder (used for ZombieBuddy) |
| `PZ_SERVER_DIR` | found in your Steam libraries | The Dedicated Server tool's folder |
| `GITHUB_TOKEN` | none | Private repos or a higher GitHub API rate limit |

## Build from source

This needs [Rust](https://rustup.rs) and Node.js 22.18 or newer (the self-checks run TypeScript directly). On Linux it also needs the [Tauri system packages](https://tauri.app/start/prerequisites/).

```
cargo test                                  # core + CLI
cd app && npm ci && npx tauri dev           # desktop app, live reload
cd app && npx tauri build                   # installers in target/release/bundle
```

How the code is organised:

| Path | What it is |
|---|---|
| `crates/core` | Shared logic. Everything specific to Project Zomboid lives in `src/pz/`; downloads (Workshop, GitHub, local) in `src/sources/`. |
| `crates/cli` | The `pzmm` command line |
| `app/` | The desktop app: Tauri 2 with Svelte 5 |

The full plan is in [docs/FOUNDATION.md](docs/FOUNDATION.md). To release, set the new `version` in `Cargo.toml` (and `app/package.json`), then push a matching `v*` tag. CI builds every download, signs the installers with the updater key (GitHub secret `TAURI_SIGNING_PRIVATE_KEY`; a copy is in the maintainer's 1Password), and publishes `latest.json`, which installed copies check for updates.

## Community

- [Discord](https://discord.gg/TfS9NwNHrH): help, ideas and bug reports
- [Ko-fi](https://ko-fi.com/sevnc3): support development

## Credits

- Icons: [Feather](https://feathericons.com) (MIT)
- Fonts: [Atkinson Hyperlegible](https://brailleinstitute.org/freefont), [IBM Plex Mono](https://github.com/IBM/plex) and [Permanent Marker](https://fonts.google.com/specimen/Permanent+Marker), all under the SIL Open Font License, via [Fontsource](https://fontsource.org)
- Steam, GitHub, Discord and Ko-fi logos via [Logo.dev](https://logo.dev); they belong to their owners
- Project Zomboid is a trademark of The Indie Stone, and Project Viewpoint belongs to its team. PZMM is a fan-made tool and isn't affiliated with either.

## License

MIT
