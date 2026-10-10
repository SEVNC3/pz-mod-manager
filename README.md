# PZ Mod Manager

A small, open-source mod manager for **Project Zomboid** (Build 41 and Build 42).

- Install mods from the **Steam Workshop** (ID or URL) or from **GitHub** (`owner/repo`, optionally `@tag` or `@branch`)
- See every mod you have: Workshop subscriptions and mods in `Zomboid/mods`
- Enable, disable and reorder mods (load order), using the same `mods/default.txt` the game reads
- Update everything you installed with one click

No Python, no account, no Steam login needed. Workshop downloads use Valve's [SteamCMD](https://developer.valvesoftware.com/wiki/SteamCMD) with an anonymous login. The app fetches SteamCMD itself the first time you install a Workshop mod.

## Download

Get `PZModManager.exe` from the [latest release](../../releases/latest) and run it. Windows may show a SmartScreen warning because the exe isn't code-signed. Click **More info → Run anyway**, or run from source (below).

## Use

1. Paste a Workshop ID/URL or a GitHub repo into the box and click **Install**. To install several at once, separate them with spaces.
2. Double-click a mod (or use **Enable / Disable**) to switch it on or off. Use **Move Up / Down** to change the load order.
3. Start the game. Your mods will be active.

Examples of what you can paste:

```
2169435993
https://steamcommunity.com/sharedfiles/filedetails/?id=2169435993
someone/their-pz-mod
someone/their-pz-mod@v1.2.0
https://github.com/someone/their-pz-mod
```

Installed mods are copied to `Zomboid/mods/<mod id>`. To remove a Steam Workshop subscription, unsubscribe in Steam. **Remove** only deletes mods in `Zomboid/mods`.

**Build 41 or 42?** The app keeps whatever format your `default.txt` already uses. If the file is new, the app uses the Build 42 format. Tick **Build 41 format** if you play B41. Before each save, the previous file is kept as `default.txt.bak`.

## Command line

The app has a command-line mode:

```
python pzmm.py add 2169435993 someone/their-pz-mod
python pzmm.py list
python pzmm.py enable ModId
python pzmm.py disable ModId
python pzmm.py remove ModId
python pzmm.py update
```

Environment variables:

| Variable | Default | Purpose |
|---|---|---|
| `PZ_HOME` | `~/Zomboid` | Your Zomboid user folder |
| `STEAMCMD_DIR` | `%LOCALAPPDATA%/pzmm/steamcmd` | Where SteamCMD lives |
| `GITHUB_TOKEN` | none | Private repos or a higher GitHub API rate limit |

## Run from source

Python 3.10+ with Tkinter, no other packages:

```
python gui.py
python test_pzmm.py
```

To build the exe: `pip install pyinstaller` then `pyinstaller --onefile --windowed --name PZModManager gui.py`. Pushing a `v*` tag builds the exe and publishes a release automatically.

## License

MIT
