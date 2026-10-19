<p align="center">
  <img src="app/static/brand/pzmm.png" alt="The PZMM logo: a skull with a red gear for an eye" width="128" height="128">
</p>

<h1 align="center">PZMM</h1>

<p align="center">
  A free mod manager, server browser and server host for Project Zomboid. Windows and Linux.<br>
  <a href="../../releases/latest"><b>Download</b></a> · <a href="#install">Install</a> · <a href="https://discord.com/invite/6zKZuZ5WWe">Discord</a> · <a href="docs/FEATURES.md">Everything it does</a>
</p>

<p align="center"><a href="https://youtu.be/MpBnMMRV-tM"><img src="docs/media/hero.gif" alt="PZMM: sorting a load order, browsing the Workshop and setting up Project Viewpoint" width="900"></a><br><a href="https://youtu.be/MpBnMMRV-tM">Watch the 2-minute video</a></p>

I made PZMM because setting up [Project Viewpoint](https://steamcommunity.com/sharedfiles/filedetails/?id=3809306528) by hand was a pain. Then it kept growing: it's a proper mod manager now, it gets you into modded servers with the right mods, and it runs dedicated servers too.

<p align="center">
  <img src="docs/media/ai-banner.jpg" alt="I use AI to help with my projects. Welcome to the future baby: a robot with glowing red eyes, a man laughing and pointing at three Reddit comments calling PZMM vibecoded" width="900">
</p>

<img src="docs/media/section-what-it-does.png" alt="What it does">

### Sort your mods, or let Autosort do it

Drag mods into groups and star the ones you care about. Autosort puts frameworks first and translations last, and keeps every `require`, `loadModAfter` and `loadModBefore`. Ctrl+Z if you don't like it.

<p align="center"><img src="docs/media/autosort.gif" alt="Dragging a mod into place, renaming a group, adding a favourite, then Autosort sorting the load order" width="720"></p>

### Every Workshop map on one grid

See which maps clash before you subscribe. It picks the biggest set that fits and writes the map order.

<p align="center"><img src="docs/media/maps.gif" alt="The Maps tab: ticking a map on shows its cells on the world map, a clash turns red, and Swap clears it" width="720"></p>

### Project Viewpoint and ZombieBuddy, set up for you

One click downloads Viewpoint and [ZombieBuddy](https://github.com/zed-0xff/ZombieBuddy), sets ZombieBuddy up and makes a separate Viewpoint profile. When a game update breaks it, Repair fixes it. Viewpoint is by ellu and norkus, ZombieBuddy by zed-0xff; PZMM just installs them.

<p align="center"><img src="docs/media/viewpoint.gif" alt="The Java mods tab: Repair ZombieBuddy fixes the launch flag, then Set up Project Viewpoint makes the Viewpoint profile" width="720"></p>

### Also

- **Join a modded server:** it downloads exactly the mods the server runs, in a profile of its own
- **The whole Workshop in the app:** search, install, and required mods come along. No Steam login
- **Keep your mods up to date:** read each update's change notes before you load a save
- **Build 42 wiped your mod list?** It puts it back
- **Find the broken mod** halves your list until the crash goes away
- Profiles, save backups, 10 languages

<img src="docs/media/section-servers.png" alt="Servers">

**New server…** installs a dedicated server with SteamCMD, with mods from a profile or a collection. Then you get a live console that points errors at the mod that caused them, restarts that warn players when a mod updates, backups, and Discord alerts, bot commands and a chat relay. It also adopts servers you already have, and talks RCON and FTP.

<a id="install"></a>
<img src="docs/media/section-install.png" alt="Install">

- **Windows:** grab `PZMM_<version>_x64-setup.exe` from the [latest release](../../releases/latest). If SmartScreen moans, click **More info**, then **Run anyway** (it isn't code-signed yet).
- **Linux:** the `.AppImage` (any distro) or the `.deb` (Debian/Ubuntu). SteamCMD needs `lib32gcc-s1`.
- **No desktop?** `pzmm-linux-x64` and `pzmm-windows-x64.exe` are the [command-line version](#for-developers).

It finds your game itself, turns your current mods into a profile called Default, and updates itself with signed updates. It only ever deletes mods it installed, and it shows you every change before joining a server or setting up ZombieBuddy.

> [!WARNING]
> Early access, so back up your saves. Only download it from [GitHub Releases](https://github.com/SEVNC3/pz-mod-manager/releases). Bugs go on [Discord](https://discord.com/invite/6zKZuZ5WWe) or in [an issue](https://github.com/SEVNC3/pz-mod-manager/issues/new?template=bug_report.yml), with **Settings > About > Copy diagnostics** pasted in.

<img src="docs/media/section-links.png" alt="Links">

[Discord](https://discord.com/invite/6zKZuZ5WWe) for help and bugs · [Nexus Mods](https://www.nexusmods.com/profile/SEVNC3) and [GitHub](https://github.com/SEVNC3) for whatever else I'm making · [Release notes](docs/release-notes/)

Free forever. If it saved you some time, there's a [Ko-fi](https://ko-fi.com/sevnc3).

<a id="for-developers"></a>
<details>
<summary><b>For developers: command line, building, releasing</b></summary>

### Command line (`pzmm`)

The command line and the app share the same profiles and data, so a headless server box can run the same setup. Add `--json` to the read commands for machine-readable output.

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
pzmm profile rename Server "Friends server"
pzmm profile duplicate Solo "Solo test"   # a copy; stays on the active profile
pzmm snapshot list           # the active profile's lists from before big changes
pzmm snapshot restore 1791000000000   # the id from snapshot list
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
pzmm server create myserver  # install a dedicated server with SteamCMD and set it up (--build legacy41 for B41)
pzmm server status myserver  # also: stop, console, log, backup, backups
pzmm servers                 # the servers PZMM manages or found
pzmm news                    # your Workshop mods updated since you last played
pzmm check-mod ./MyMod       # check a mod or upload folder before uploading
pzmm watch servertest --every 10 --on-update "restart-server.bat"   # check every 10 minutes; run a script on updates
```

| Variable | Default | Purpose |
|---|---|---|
| `PZ_HOME` | `~/Zomboid` | Your Zomboid user folder |
| `PZMM_DATA` | `%LOCALAPPDATA%\pzmm` or `~/.local/share/pzmm` | Profiles, backups, SteamCMD |
| `PZ_GAME_DIR` | found in your Steam libraries | The game's install folder (used for ZombieBuddy) |
| `PZ_SERVER_DIR` | found in your Steam libraries | The Dedicated Server tool's folder |
| `GITHUB_TOKEN` | none | Private repos or a higher GitHub API rate limit |

### Build from source

This needs [Rust](https://rustup.rs) and Node.js 22.18 or newer (the self-checks run TypeScript directly). On Linux it also needs the [Tauri system packages](https://tauri.app/start/prerequisites/).

```
cargo test                                  # core + CLI
cd app && npm ci && npx tauri dev           # desktop app, live reload
cd app && npx tauri build                   # installers in target/release/bundle
```

| Path | What it is |
|---|---|
| `crates/core` | Shared logic. Everything specific to Project Zomboid lives in `src/pz/`; downloads (Workshop, GitHub, local) in `src/sources/`. |
| `crates/cli` | The `pzmm` command line |
| `app/` | The desktop app: Tauri 2 with Svelte 5 |

The full plan is in [docs/FOUNDATION.md](docs/FOUNDATION.md). To release, set the new `version` in `Cargo.toml` (and `app/package.json`), then push a matching `v*` tag. CI builds every download, signs the installers with the updater key (GitHub secret `TAURI_SIGNING_PRIVATE_KEY`; a copy is in the maintainer's 1Password), and publishes `latest.json`, which installed copies check for updates. The release page starts with `docs/release-notes/install.md`, then the bug-report note (`docs/release-notes/bug-report.md`), then `docs/release-notes/<tag>.md`.

</details>

### Credits

- [Anthropic](https://www.anthropic.com) for [Claude Code](https://claude.com/claude-code)
- Icons: [Feather](https://feathericons.com) (MIT)
- Fonts: [Atkinson Hyperlegible](https://brailleinstitute.org/freefont), [IBM Plex Mono](https://github.com/IBM/plex) and [Permanent Marker](https://fonts.google.com/specimen/Permanent+Marker), all under the SIL Open Font License, via [Fontsource](https://fontsource.org)
- Steam, GitHub, Discord and Ko-fi logos via [Logo.dev](https://logo.dev); they belong to their owners
- ZombieBuddy logo from [ZombieBuddy](https://github.com/zed-0xff/ZombieBuddy) by Andrey "Zed" Zaikin (MIT)
- Server countries (off unless you turn them on): [country.is](https://country.is), which uses MaxMind GeoLite2 data
- Project Zomboid and its art are The Indie Stone's, and Project Viewpoint belongs to its team. PZMM is a fan-made tool and isn't affiliated with either.

MIT licensed.
