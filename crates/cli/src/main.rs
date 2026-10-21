use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use pzmm_core::profile::{self, Profile};
use pzmm_core::pz::Paths;
use pzmm_core::sources::{self, Source};
use pzmm_core::store;

/// PZMM - Project Zomboid Mod Manager
#[derive(Parser)]
#[command(
    version,
    after_help = "Snapshots: before updates, removals and imports (and the app's Autosort and grouping) PZMM keeps the profile's last 10 lists: pzmm snapshot list, pzmm snapshot restore <id>.\nEnv: PZ_HOME (default ~/Zomboid), PZMM_DATA, PZ_GAME_DIR, PZ_SERVER_DIR, GITHUB_TOKEN. They win over the app's Settings (settings.json in the config folder).
Only in the app: the server browser and joining servers, Maps, browsing the Workshop, save tools, and setting up a server's schedules, Discord and FTP (pzmm server start runs what's set up)."
)]
struct Cli {
    /// Print JSON instead of text: list, doctor, errors, news, outdated, servers, mode, profile
    /// list, snapshot list, server status and server backups (messages go to stderr)
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Install from Workshop id/URL, GitHub owner/repo[@ref] or URL, or a local zip/folder
    Add {
        #[arg(required = true)]
        sources: Vec<String>,
        /// Install without enabling
        #[arg(long)]
        no_enable: bool,
    },
    /// List mods (enabled first, in load order)
    List,
    Enable {
        #[arg(required = true)]
        ids: Vec<String>,
    },
    Disable {
        #[arg(required = true)]
        ids: Vec<String>,
    },
    /// Delete mods PZMM installed
    Remove {
        #[arg(required = true)]
        ids: Vec<String>,
    },
    /// Download what changed of what PZMM installed (Workshop updates; GitHub and local sources again)
    Update,
    /// Restore a mod's previous version (run again to undo)
    Rollback { id: String },
    /// Manage profiles (named mod setups)
    #[command(subcommand)]
    Profile(ProfileCmd),
    /// The active profile's snapshots (its lists from before big changes)
    #[command(subcommand)]
    Snapshot(SnapshotCmd),
    /// Force the default.txt format: 41 or 42 (default: keep the file's format)
    Build { build: u8 },
    /// Search the Steam Workshop (no query: most popular this week)
    Search {
        query: Vec<String>,
        /// trend, toprated, totaluniquesubscribers, mostrecent, lastupdated
        #[arg(long, default_value = "trend")]
        sort: String,
        #[arg(long, default_value_t = 1)]
        page: u32,
        /// Only items tagged for this build, e.g. 42
        #[arg(long)]
        build: Option<u8>,
    },
    /// Find Workshop items that provide a mod id (e.g. one a mod requires)
    Find {
        mod_id: String,
        /// Install the best confirmed match
        #[arg(long)]
        install: bool,
    },
    /// Check the active profile and server configs; exits with 1 if anything is broken
    Doctor,
    /// The first real error in the last game and server logs, and errors counted per mod
    Errors,
    /// List mods PZMM installed from the Workshop that have updates waiting
    Outdated,
    /// The dedicated servers PZMM manages or found, with their status
    Servers,
    /// Your Workshop mods updated since you last played, with their change notes
    News,
    /// Check a mod you're making, or its Workshop upload folder, the way the game and its uploader will
    CheckMod { dir: std::path::PathBuf },
    /// Dedicated servers: create (install and set up), start (runs here: type commands, `quit`
    /// stops it; its schedules, update checks and crash watch run while it does), stop, status,
    /// console (commands over RCON), log, backup, backups; export (profile -> Server/<name>.ini)
    /// or import (ini -> profile)
    Server {
        /// create, start, stop, status, console, log, backup, backups, export or import
        action: String,
        name: String,
        /// console: the command to send (none: read commands line by line)
        command: Vec<String>,
        /// create: legacy41 for Build 41 (default: Build 42)
        #[arg(long, default_value = "b42")]
        build: String,
        /// create: where to install the server (default: <PZMM data>/servers/<build>)
        #[arg(long)]
        dir: Option<std::path::PathBuf>,
        /// create: a data folder of its own (default: the Zomboid folder)
        #[arg(long)]
        data: Option<std::path::PathBuf>,
        /// create: the game port (the direct port is the next one)
        #[arg(long, default_value_t = 16261)]
        port: u16,
        /// create: Java memory in MB (default: half the RAM, 2-8 GB)
        #[arg(long)]
        memory: Option<u32>,
    },
    /// Watch a server's Workshop items and react when one updates (players get "version is
    /// different" until the server restarts). Runs until stopped.
    Watch {
        /// Server config name (Server/<name>.ini)
        server: String,
        /// Minutes between checks
        #[arg(long, default_value_t = 10)]
        every: u64,
        /// Shell command to run when items update, e.g. your restart script
        #[arg(long)]
        on_update: Option<String>,
    },
    /// Show or switch the game: zomboid (plain) or viewpoint (Project Viewpoint, 3D)
    Mode {
        /// zomboid or viewpoint; omit to show the current mode
        to: Option<String>,
        /// Only list what switching would change
        #[arg(long)]
        dry_run: bool,
    },
    /// Set up the ZombieBuddy Java agent (Java mods): status, install or uninstall
    Zombiebuddy {
        #[arg(default_value = "status")]
        action: String,
    },
    /// Read the map cells (both builds) of every Map-tagged Workshop item from Steam's file lists
    /// (nothing is downloaded or installed); only new and updated items are read again
    MapScan {
        /// Then write the results as a gzipped seed index (crates/core/data/map-seed.json.gz)
        #[arg(long)]
        seed: Option<std::path::PathBuf>,
    },
    /// Read the mod IDs of every Workshop item from Steam's file lists (and the small mod.info
    /// files where the description doesn't confirm a folder); only items updated since are read again
    ModIndex {
        /// Then write the index as a gzipped seed (crates/core/data/mod-index.json.gz)
        #[arg(long)]
        seed: Option<std::path::PathBuf>,
    },
}

#[derive(Subcommand)]
enum ProfileCmd {
    List,
    /// Create an empty profile for the same game as the active one (duplicate copies one)
    New { name: String },
    /// Switch to a profile and apply it to the game
    Use { name: String },
    /// Write the active profile as a shareable JSON file (stdout without a file)
    Export { file: Option<std::path::PathBuf> },
    /// Install what a shared profile needs from the Workshop, then use it
    Import { file: std::path::PathBuf },
    Delete { name: String },
    /// Rename a profile (the active one too)
    Rename { from: String, to: String },
    /// Copy a profile under a new name (stays on the active one)
    Duplicate { from: String, to: String },
}

#[derive(Subcommand)]
enum SnapshotCmd {
    /// Newest first: id, when, why, mods enabled
    List,
    /// Put the active profile back as snapshot <id> had it (its current list is snapshotted first)
    Restore { id: String },
}

fn main() {
    let cli = Cli::parse();
    if cli.json {
        // stdout carries only the JSON
        pzmm_core::set_log_sink(|l| eprintln!("{}", l.text));
    }
    if let Err(e) = run(cli) {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

/// Print `v` as JSON (for --json).
fn print_json(v: serde_json::Value) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(&v)?);
    Ok(())
}

/// Apply `p` with its maps from one fresh mod scan (unreadable mods: the `maps` block is kept).
fn apply(paths: &Paths, p: &Profile) -> Result<()> {
    profile::apply(paths, p, profile::maps_scan(store::scan(paths)).as_ref())
}

fn run(cli: Cli) -> Result<()> {
    let paths = Paths::detect();
    pzmm_core::sweep_temp(&paths);
    // read-only commands skip the lock; the rest wait for the app to finish its change
    // hosting keeps its own lock, only while it writes servers.json (a server runs for hours)
    let hosting = matches!(&cli.cmd, Cmd::Server { action, .. } if !["export", "import"].contains(&action.as_str()));
    let read_only = hosting || matches!(cli.cmd, Cmd::Search { .. } | Cmd::Watch { .. } | Cmd::MapScan { .. } | Cmd::News | Cmd::CheckMod { .. } | Cmd::ModIndex { .. } | Cmd::Servers);
    let json = cli.json;
    let _lock = if read_only { None } else { Some(pzmm_core::lock_data(&paths)?) };
    // sync can save the profile, so only commands holding the lock run it
    let (mut p, adopted) = if read_only { (profile::peek(&paths), false) } else { profile::sync(&paths)? };
    if adopted {
        eprintln!("picked up mod list changes made in-game (profile {:?})", p.name);
    }
    match cli.cmd {
        Cmd::Add { sources: s, no_enable } => {
            let parsed: Vec<Source> = s.iter().map(|s| Source::parse(s)).collect();
            let ids = sources::install(&paths, &parsed, true);
            if ids.is_empty() {
                bail!("nothing was installed");
            }
            if !no_enable {
                ids.iter().for_each(|i| p.set(i, true));
                apply(&paths, &p)?;
            }
        }
        Cmd::Enable { ids } => {
            let mods = store::scan(&paths)?;
            let mut done = 0;
            for id in &ids {
                if mods.contains_key(id) {
                    p.set(id, true);
                    done += 1;
                } else {
                    let like = mods.keys().find(|k| k.eq_ignore_ascii_case(id)).map(|k| format!(" (did you mean {k}?)")).unwrap_or_default();
                    eprintln!("{id}: not installed, skipped{like}");
                }
            }
            if done == 0 {
                bail!("nothing enabled");
            }
            apply(&paths, &p)?;
        }
        Cmd::Disable { ids } => {
            let mods = store::scan(&paths)?;
            // an id that isn't installed has nothing to keep off: it leaves the profile
            ids.iter().for_each(|i| if mods.contains_key(i) { p.set(i, false) } else { p.forget(i) });
            apply(&paths, &p)?;
        }
        Cmd::Remove { ids } => {
            profile::snapshot(&paths, &p.name, "remove")?;
            let refused = store::remove(&paths, &ids, pzmm_core::settings::load().recycle)?;
            for id in &refused {
                eprintln!("{id}: not installed by PZMM (Workshop subscription or manual install); left alone");
            }
            if refused.len() == ids.len() {
                bail!("nothing removed");
            }
            ids.iter().filter(|i| !refused.contains(i)).for_each(|i| p.forget(i));
            apply(&paths, &p)?;
        }
        Cmd::Update => {
            // Workshop items that changed; GitHub and local sources can't be asked, so they all come again
            // (held mods are left out: sources::fetch skips them)
            let outdated = sources::check_updates(&paths)?.outdated;
            let mut seen = std::collections::HashSet::new();
            let all: Vec<Source> = store::load(&paths)?
                .into_iter()
                .filter(|(id, e)| (!e.source.starts_with("workshop:") || outdated.contains(id)) && seen.insert(e.source.clone()))
                .filter_map(|(_, e)| Source::from_index(&e.source))
                .collect();
            if all.is_empty() {
                println!("everything PZMM installed is up to date");
                return Ok(());
            }
            profile::snapshot(&paths, &p.name, "update")?;
            if sources::update(&paths, &all).is_empty() {
                bail!("nothing was updated");
            }
            apply(&paths, &p)?; // updates can add or rename map folders
        }
        Cmd::Rollback { id } => {
            store::rollback(&paths, &id)?;
            apply(&paths, &p)?;
            println!("{id}: restored previous version");
        }
        Cmd::List => list(&paths, &p, json)?,
        Cmd::MapScan { seed } => {
            use pzmm_core::pz::maps;
            let start = std::time::Instant::now();
            let mut last = "";
            let p = maps::scan_workshop(&paths, &std::sync::atomic::AtomicBool::new(false), &mut |p| {
                if p.phase != last || p.done % 25 == 0 {
                    eprintln!("{:>6.0}s {} {}/{} ({} failed)", start.elapsed().as_secs_f32(), p.phase, p.done, p.total, p.failed);
                    last = p.phase;
                }
            })?;
            println!("read {} items in {:.0}s ({} failed)", p.done, start.elapsed().as_secs_f32(), p.failed);
            let all = maps::checked(&paths);
            for build in [41, 42] {
                let items = all.values().filter(|c| c.builds.get(&build).is_some_and(|m| !m.is_empty()));
                let mods: usize = items.clone().map(|c| c.builds[&build].len()).sum();
                println!("Build {build}: {} Workshop items with maps, {mods} map mods", items.count());
            }
            if let Some(file) = seed {
                let n = maps::write_seed(&paths, &file)?;
                println!("wrote {n} items to {} ({} KB)", file.display(), std::fs::metadata(&file)?.len() / 1024);
            }
        }
        Cmd::ModIndex { seed } => {
            use pzmm_core::pz::mod_index;
            let start = std::time::Instant::now();
            let (mut last, mut said) = ("", start);
            let p = mod_index::scan(&paths, &std::sync::atomic::AtomicBool::new(false), &mut |p| {
                if p.phase != last || said.elapsed().as_secs() >= 30 {
                    eprintln!("{:>6.0}s {} {}/{} ({} failed)", start.elapsed().as_secs_f32(), p.phase, p.done, p.total, p.failed);
                    (last, said) = (p.phase, std::time::Instant::now());
                }
            })?;
            println!("read {} items in {:.0}s ({} failed)", p.done, start.elapsed().as_secs_f32(), p.failed);
            if let Some(file) = seed {
                let (items, ids) = mod_index::write_seed(&paths, &file)?;
                println!("wrote {items} items, {ids} mod IDs to {} ({} KB)", file.display(), std::fs::metadata(&file)?.len() / 1024);
            }
        }
        Cmd::Search { query, sort, page, build } => {
            let tag = build.map(|b| format!("Build {b}"));
            let q = sources::steam_workshop::Query { search: query.join(" "), sort, page, required: tag.into_iter().collect(), ..Default::default() };
            let res = sources::steam_workshop::browse(pzmm_core::pz::STEAM_APP, &q)?;
            println!("{} results (page {page})", res.total);
            for i in res.items {
                println!("{:>11}  {:<50} {:>9} subs", i.id, i.title.chars().take(50).collect::<String>(), i.subscriptions);
            }
        }
        Cmd::Find { mod_id, install } => {
            let found = sources::find_mod(&mod_id, None, profile::effective_build(&paths))?;
            for c in found.iter().take(8) {
                let tag = if c.sure() { "match" } else if c.confirmed { "other" } else { "     " };
                println!("{tag} {:>11}  {:<50} {:>9} subs", c.item.id, c.item.title.chars().take(50).collect::<String>(), c.item.subscriptions);
            }
            match found.first() {
                Some(best) if install && best.sure() => {
                    let ids = sources::install(&paths, &[Source::Workshop(best.item.id.clone())], true);
                    if ids.is_empty() {
                        bail!("couldn't install workshop:{}", best.item.id);
                    }
                    ids.iter().for_each(|i| p.set(i, true));
                    apply(&paths, &p)?;
                }
                _ if install => bail!("no Workshop item clearly provides {mod_id}; install one of the above by id"),
                _ => {}
            }
        }
        Cmd::Doctor => {
            use pzmm_core::pz::doctor;
            let mods = store::scan(&paths)?;
            let mut found = doctor::check(&paths, &p.enabled(), &mods, profile::effective_build(&paths));
            found.extend(doctor::check_workshop(&paths, &p.enabled(), &mods));
            if json {
                print_json(serde_json::to_value(&found)?)?;
            }
            for f in found.iter().filter(|_| !json) {
                let tag = match f.level {
                    doctor::Level::Error => "error  ",
                    doctor::Level::Warning => "warning",
                    doctor::Level::Note => "note   ",
                };
                println!("{tag} {}: {}", f.subject, f.text);
            }
            let errors = found.iter().filter(|f| f.level == doctor::Level::Error).count();
            let warnings = found.iter().filter(|f| f.level == doctor::Level::Warning).count();
            if !json {
                println!("{errors} error(s), {warnings} warning(s)");
            }
            if errors > 0 {
                std::process::exit(1);
            }
        }
        Cmd::Errors => {
            let reports = pzmm_core::pz::logs::reports(&paths, &store::scan(&paths)?);
            if json {
                return print_json(serde_json::to_value(&reports)?);
            }
            if reports.is_empty() {
                println!("no errors in {}", paths.zomboid.join("console.txt").display());
            }
            for r in reports {
                println!("{}", r.file);
                if let Some(f) = r.first {
                    println!("  first error, line {} (points at: {}):", f.line, if f.ids.is_empty() { "no installed mod".into() } else { f.ids.join(", ") });
                    f.text.lines().for_each(|l| println!("    {l}"));
                }
                if !r.mods.is_empty() {
                    println!("\n  errors per mod:");
                }
                for e in r.mods {
                    println!("{:>5}  {}{}  (first: {})", e.count, e.name, e.id.map(|i| format!(" [{i}]")).unwrap_or_default(), e.example);
                }
            }
        }
        Cmd::CheckMod { dir } => {
            let tags = pzmm_core::pz::authoring::allowed_tags(pzmm_core::pz::zombiebuddy::game_dir().as_deref());
            let found = pzmm_core::pz::authoring::check(&dir, &tags);
            for f in &found {
                println!("{}: {}: {}", if f.level == pzmm_core::pz::doctor::Level::Error { "error" } else { "warning" }, f.subject, f.text);
            }
            if found.is_empty() {
                println!("no problems found");
            }
            if found.iter().any(|f| f.level == pzmm_core::pz::doctor::Level::Error) {
                std::process::exit(1);
            }
        }
        Cmd::News => {
            let feed = pzmm_core::pz::feed::feed(&paths, &pzmm_core::store::scan(&paths)?, true)?;
            if json {
                return print_json(serde_json::to_value(&feed)?);
            }
            println!("{} updated since {} ({})", feed.entries.len(), ago(feed.since), if feed.basis.is_empty() { "last week" } else { feed.basis });
            for e in &feed.entries {
                let risks = if e.risks.is_empty() { String::new() } else { format!(" [may break saves: {}]", e.risks.join(", ")) };
                println!("\n{} ({}){risks}", e.title, e.mods.join(", "));
                e.notes.iter().for_each(|n| println!("  {}", n.text.replace('\n', "\n  ")));
                // the mod's own changelog, when Steam has no notes
                e.bundled.iter().flat_map(|b| &b.entries).for_each(|l| println!("  [{}]\n    {}", l.title, l.text.replace('\n', "\n    ")));
            }
            if !feed.error.is_empty() {
                eprintln!("error: some change notes didn't load: {}", feed.error);
            }
        }
        Cmd::Outdated => {
            let found = sources::check_updates(&paths)?;
            if json {
                return print_json(serde_json::to_value(&found.outdated)?); // the list it always was
            }
            if found.outdated.is_empty() && found.held.is_empty() {
                println!("everything PZMM installed is up to date");
            }
            found.outdated.iter().for_each(|i| println!("{i}: update available (pzmm update)"));
            found.held.keys().for_each(|i| println!("{i}: update available, but its updates are held"));
        }
        Cmd::Server { action, name, command, build, dir, data, port, memory } => match action.as_str() {
            "export" | "import" => server(&paths, &p, &action, &name)?,
            _ => host(&paths, &action, &name, &command.join(" "), json, pzmm_core::pz::host::NewServer {
                name: name.clone(),
                build,
                install_dir: dir.map(std::path::absolute).transpose()?,
                cachedir: data.map(std::path::absolute).transpose()?,
                game_port: port,
                direct_port: port.wrapping_add(1),
                memory_mb: memory.unwrap_or_else(|| pzmm_core::pz::host::default_memory_mb(pzmm_core::pz::host::total_ram_mb())),
                max_players: 32,
                ..Default::default()
            })?,
        },
        Cmd::Servers => {
            let cards = pzmm_core::pz::host::cards(&paths);
            if json {
                return print_json(serde_json::to_value(&cards)?);
            }
            if cards.is_empty() {
                println!("no servers yet (pzmm server create <name>)");
            }
            for c in cards {
                println!("{:<24} {:<10} {}{}", c.name, format!("{:?}", c.status), c.ini.display(), if c.managed { "" } else { " (not managed by PZMM)" });
            }
        }
        Cmd::Watch { server, every, on_update } => watch(&paths, &server, every, on_update.as_deref())?,
        Cmd::Mode { to, dry_run } => {
            use pzmm_core::pz::viewpoint as vp;
            let Some(to) = to else {
                if json {
                    return print_json(serde_json::json!({ "mode": profile::kind_of(&paths, &p.name), "profile": p.name }));
                }
                println!("{} (profile {:?})", profile::kind_of(&paths, &p.name), p.name);
                return Ok(());
            };
            if to != vp::KIND && to != profile::DEFAULT_KIND {
                bail!("expected zomboid or viewpoint");
            }
            for line in vp::preview(&paths, &to, &store::scan(&paths)?)? {
                println!("{} {}", if dry_run { "would" } else { "-" }, pzmm_core::unkeyed(&line));
            }
            if !dry_run {
                if to == vp::KIND {
                    sources::install(&paths, &vp::needed(&paths, &store::scan(&paths)?)?, false);
                    vp::enter(&paths, &store::scan(&paths)?)?;
                } else {
                    vp::leave(&paths)?;
                }
            }
        }
        Cmd::Zombiebuddy { action } => zombiebuddy(&paths, &mut p, &action)?,
        Cmd::Profile(ProfileCmd::List) => {
            if json {
                return print_json(serde_json::to_value(profile::list(&paths).into_iter().map(|n| serde_json::json!({ "name": n, "active": n == p.name })).collect::<Vec<_>>())?);
            }
            for name in profile::list(&paths) {
                println!("{} {name}", if name == p.name { '*' } else { ' ' });
            }
        }
        Cmd::Profile(ProfileCmd::New { name }) => {
            if profile::exists(&paths, &name) {
                bail!("profile {name:?} already exists");
            }
            // empty, for the same game as the active one, as in the app (duplicate copies a profile)
            use pzmm_core::pz::viewpoint as vp;
            let viewpoint = profile::kind_of(&paths, &p.name) == vp::KIND;
            profile::save(&paths, &if viewpoint { vp::fresh(&name) } else { Profile::parse(&name, "") })?;
            profile::set_kind(&paths, &name, if viewpoint { vp::KIND } else { profile::DEFAULT_KIND })?;
            println!("created the empty profile {name:?} (switch with: pzmm profile use {name})");
        }
        Cmd::Profile(ProfileCmd::Use { name }) => {
            let p = profile::load(&paths, &name)?;
            apply(&paths, &p)?;
            println!("now using {:?}", p.name);
        }
        Cmd::Profile(ProfileCmd::Export { file }) => {
            let json = serde_json::to_string_pretty(&profile::export(&paths, &store::scan(&paths)?)?)?;
            match file {
                Some(f) => std::fs::write(&f, json)?,
                None => println!("{json}"),
            }
        }
        Cmd::Profile(ProfileCmd::Import { file }) => {
            let shared: profile::Shared = serde_json::from_str(&std::fs::read_to_string(&file)?)?;
            let needs = profile::import_needs(&shared, &store::scan(&paths)?);
            let installed = sources::install(&paths, &needs, false);
            let p = profile::import(&paths, &shared)?;
            println!("now using {:?}", p.name);
            if !needs.is_empty() && installed.is_empty() {
                bail!("none of the {} mod source(s) it needs installed", needs.len());
            }
        }
        Cmd::Profile(ProfileCmd::Delete { name }) => profile::delete(&paths, &name)?,
        Cmd::Profile(ProfileCmd::Rename { from, to }) => {
            profile::rename(&paths, &from, to.trim())?;
            println!("renamed {from:?} to {:?}", to.trim());
        }
        Cmd::Profile(ProfileCmd::Duplicate { from, to }) => {
            profile::duplicate(&paths, &from, to.trim())?;
            println!("copied {from:?} as {:?}", to.trim());
        }
        Cmd::Snapshot(SnapshotCmd::List) => {
            let snaps = profile::snapshots(&paths, &p.name);
            if json {
                return print_json(serde_json::to_value(&snaps)?);
            }
            if snaps.is_empty() {
                println!("no snapshots of {:?} yet", p.name);
            }
            for s in snaps {
                println!("{:>14}  {:<12} {:<9} {} of {} mods enabled", s.id, ago(s.time), s.label, s.enabled, s.mods);
            }
        }
        Cmd::Snapshot(SnapshotCmd::Restore { id }) => {
            apply(&paths, &profile::restore(&paths, &p.name, &id)?)?;
            println!("restored {:?} from snapshot {id} (the list it replaced is a snapshot too)", p.name);
        }
        Cmd::Build { build } => profile::set_b41(&paths, Some(match build {
            41 => true,
            42 => false,
            _ => bail!("build must be 41 or 42"),
        }))
        .and_then(|_| apply(&paths, &p))?,
    }
    Ok(())
}

/// How long ago Unix time secs was, roughly.
fn ago(secs: u64) -> String {
    match store::now().saturating_sub(secs) {
        d if d < 3600 => format!("{} min ago", d / 60),
        d if d < 86_400 => format!("{} h ago", d / 3600),
        d => format!("{} days ago", d / 86_400),
    }
}

/// Enabled mods in load order, then the rest of the library.
fn list(paths: &Paths, p: &Profile, json: bool) -> Result<()> {
    let mut mods = store::scan(paths)?;
    // (id, name, source, enabled, installed)
    let mut rows: Vec<(String, String, String, bool, bool)> = vec![];
    for id in p.enabled() {
        match mods.remove(&id) {
            Some(m) => rows.push((m.id, m.name, m.source, true, true)),
            None => rows.push((id, String::new(), String::new(), true, false)),
        }
    }
    for line in &p.lines {
        if let profile::Line::Mod { id, on: false } = line
            && let Some(m) = mods.remove(id)
        {
            rows.push((m.id, m.name, m.source, false, true));
        }
    }
    rows.extend(mods.into_values().map(|m| (m.id, m.name, m.source, false, true)));
    if json {
        let mods: Vec<serde_json::Value> = rows.into_iter().map(|(id, name, source, enabled, installed)| serde_json::json!({ "id": id, "name": name, "source": source, "enabled": enabled, "installed": installed })).collect();
        return print_json(serde_json::json!({ "profile": p.name, "mods": mods }));
    }
    println!("profile: {}", p.name);
    for (id, name, source, enabled, installed) in rows {
        let name = if installed { name } else { "(missing)".into() };
        println!("[{}] {id:30} {name:40} {source}", if enabled { 'x' } else { ' ' });
    }
    Ok(())
}

fn zombiebuddy(paths: &Paths, p: &mut Profile, action: &str) -> Result<()> {
    use pzmm_core::pz::zombiebuddy as zb;
    let launcher = zb::game_dir().and_then(|g| zb::launcher_dir(&g));
    let libs = || store::scan(paths).ok().and_then(|m| zb::locate(&m).1);
    match action {
        "status" => {
            let l = launcher.as_deref();
            println!("game folder:     {}", l.map_or("not found (set PZ_GAME_DIR)".into(), |d| d.display().to_string()));
            println!("ZombieBuddy mod: {}", libs().map_or("not installed".into(), |d| d.display().to_string()));
            println!("agent files:     {}", l.zip(libs()).is_some_and(|(l, s)| zb::files_current(l, &s)));
            println!("launcher flag:   {}", l.is_some_and(zb::launcher_patched));
            println!("enabled:         {}", p.enabled().iter().any(|i| i == zb::MOD_ID));
        }
        "install" => {
            let Some(l) = launcher else { bail!("Project Zomboid's game folder wasn't found; set PZ_GAME_DIR") };
            if libs().is_none() {
                sources::install(paths, &[Source::Workshop(zb::WORKSHOP_ID.into())], false);
            }
            let Some(s) = libs() else { bail!("couldn't get the ZombieBuddy mod") };
            for c in zb::install(&l, &s, false)? {
                println!("{}", pzmm_core::unkeyed(&c));
            }
            p.set(zb::MOD_ID, true);
            apply(paths, p)?;
            println!("ZombieBuddy is set up. Java mods will ask for your approval the first time they load.");
        }
        "uninstall" => {
            let Some(l) = launcher else { bail!("Project Zomboid's game folder wasn't found; set PZ_GAME_DIR") };
            for c in zb::uninstall(&l, false)? {
                println!("{}", pzmm_core::unkeyed(&c));
            }
        }
        _ => bail!("expected status, install or uninstall"),
    }
    Ok(())
}

fn server(paths: &Paths, p: &Profile, action: &str, name: &str) -> Result<()> {
    use pzmm_core::pz::server;
    let file = server::path(paths, name)?;
    match action {
        "export" => {
            if !file.exists() {
                bail!("there's no server config {} (pzmm server create {name} makes one)", file.display());
            }
            for note in server::export(paths, name, &p.enabled(), &store::scan(paths)?, None)? {
                println!("{note}");
            }
        }
        _ => {
            // import (the only other action routed here)
            let cfg = server::read(&file)?;
            let have = sources::workshop_available(paths);
            let missing: Vec<Source> = cfg.workshop.iter().filter(|w| !have.contains(*w)).map(|w| Source::Workshop(w.clone())).collect();
            sources::install(paths, &missing, false);
            let prof = profile::for_server(paths, name, &format!("ini:{name}"), cfg.mods, &cfg.maps)?;
            apply(paths, &prof)?;
            println!("now using profile {:?}", prof.name);
        }
    }
    Ok(())
}

fn host(paths: &Paths, action: &str, name: &str, command: &str, json: bool, mut new: pzmm_core::pz::host::NewServer) -> Result<()> {
    use pzmm_core::pz::host::{self, Status};
    use std::io::BufRead;
    match action {
        "create" => {
            new.admin_user = "admin".into();
            new.admin_password = host::generate_password();
            let s = host::create(paths, &new, &std::sync::atomic::AtomicBool::new(false))?;
            println!("created {name}: {} (installed in {})", s.ini(paths).display(), s.install_dir.display());
            println!("admin user: admin, password: {} (shown once; PZMM keeps it in the system's credential store, or in secrets.json in its data folder when there's none)", new.admin_password);
            println!("UPnP is off: forward UDP {}-{} on your router for players outside your network", new.game_port, new.direct_port);
            println!("mods: pzmm server export {name} writes the active profile's; then pzmm server start {name}");
        }
        "start" => {
            host::set_event_sink(|e| {
                if let Some(l) = &e.line {
                    println!("{}", l.text);
                    if let Some(n) = &l.note {
                        println!("PZMM: {}", n.text);
                    }
                }
            });
            host::start(paths, name, None)?;
            println!("PZMM: type server commands here; `quit` saves and stops it. If this window closes, the server keeps running: pzmm server stop {name}");
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                for line in std::io::stdin().lock().lines().map_while(Result::ok) {
                    let _ = tx.send(line);
                }
            });
            let mut tick = std::time::Instant::now();
            loop {
                if !host::attached(name) {
                    // a crash: crash watch may start it again
                    if host::crashed(name).is_none() {
                        break;
                    }
                    pzmm_core::pz::autopilot::tick_only(paths, name);
                    if !host::attached(name) {
                        break;
                    }
                }
                // this server's automation only: the app (or another pzmm) runs the others'
                if tick.elapsed() > std::time::Duration::from_secs(5) {
                    pzmm_core::pz::autopilot::tick_only(paths, name);
                    tick = std::time::Instant::now();
                }
                if let Ok(line) = rx.recv_timeout(std::time::Duration::from_millis(300))
                    && !line.trim().is_empty()
                    && let Err(e) = host::send(paths, name, line.trim())
                {
                    eprintln!("error: {e:#}");
                }
            }
            let card = host::cards(paths).into_iter().find(|c| c.name == name);
            if let Some(r) = card.and_then(|c| c.reason) {
                println!("PZMM: {}", r.text);
            }
        }
        "stop" => {
            host::stop(paths, name, std::time::Duration::from_secs(90))?;
            println!("{name} stopped");
        }
        "status" => {
            let Some(c) = host::cards(paths).into_iter().find(|c| c.name == name) else { bail!("no server named {name}") };
            if json {
                let players = if c.status == Status::Running { host::players(paths, name).ok() } else { None };
                return print_json(serde_json::json!({ "server": serde_json::to_value(&c)?, "players": players }));
            }
            println!("{name}: {:?}{}", c.status, if c.managed { "" } else { " (not managed by PZMM)" });
            println!("config: {}", c.ini.display());
            println!("ports: UDP {} and {}", c.game_port, c.direct_port);
            if c.status == Status::Running {
                println!("memory: {} MB of {} MB", c.memory_used >> 20, c.memory_mb);
                match host::players(paths, name) {
                    Ok(p) => println!("players ({}): {}", p.len(), p.join(", ")),
                    Err(e) => println!("players: {e:#}"),
                }
            }
        }
        "console" => {
            let failed = std::cell::Cell::new(false);
            let run = |cmd: &str| match host::send(paths, name, cmd) {
                Ok(reply) => println!("{}", reply.unwrap_or_default().trim_end()),
                Err(e) => {
                    eprintln!("error: {e:#}");
                    failed.set(true);
                }
            };
            if command.is_empty() {
                for line in std::io::stdin().lock().lines().map_while(Result::ok).filter(|l| !l.trim().is_empty()) {
                    run(line.trim());
                }
            } else {
                run(command);
            }
            if failed.get() {
                bail!("a command failed");
            }
        }
        "log" => {
            for l in host::log(paths, name, 0).1.iter().rev().take(200).rev() {
                println!("{}", l.text);
                if let Some(n) = &l.note {
                    println!("PZMM: {}", n.text);
                }
            }
        }
        "backup" => {
            let s = host::find(paths, name).with_context(|| format!("PZMM doesn't manage a server named {name}"))?;
            let b = pzmm_core::pz::server_backup::create(paths, &s, "manual")?;
            println!("{}", b.file.display());
        }
        "backups" => {
            if json {
                return print_json(serde_json::to_value(pzmm_core::pz::server_backup::list(paths, name))?);
            }
            for b in pzmm_core::pz::server_backup::list(paths, name) {
                println!("{} ({} MB, {})", b.file.display(), b.size >> 20, b.reason);
            }
        }
        _ => bail!("expected create, start, stop, status, console, log, backup, backups, export or import"),
    }
    Ok(())
}

fn watch(paths: &Paths, name: &str, every: u64, on_update: Option<&str>) -> Result<()> {
    use pzmm_core::pz::server;
    use pzmm_core::sources::steam_workshop;
    let file = server::path(paths, name)?;
    let mut seen: std::collections::HashMap<String, u64> = Default::default();
    println!("watching the Workshop items of {} every {every} min (Ctrl+C stops)", file.display());
    loop {
        // a config being edited may not read for a moment: say so and look again next time
        let cfg = server::read(&file).unwrap_or_else(|e| {
            eprintln!("error: reading {}: {e:#}", file.display());
            Default::default()
        });
        let ids: Vec<&str> = cfg.workshop.iter().map(String::as_str).collect();
        // Steam answers at most 100 items per request
        match ids.chunks(100).map(steam_workshop::details).collect::<Result<Vec<_>>>().map(|d| d.concat()) {
            Ok(items) => {
                let changed: Vec<String> =
                    steam_workshop::newly_updated(&mut seen, items).into_iter().map(|i| format!("{} ({})", i.title, i.id)).collect();
                if !changed.is_empty() {
                    println!("updated on the Workshop: {}", changed.join(", "));
                    if let Some(cmd) = on_update {
                        let status = if cfg!(windows) {
                            std::process::Command::new("cmd").args(["/C", cmd]).status()
                        } else {
                            std::process::Command::new("sh").args(["-c", cmd]).status()
                        };
                        println!("ran --on-update: {}", status.map_or_else(|e| e.to_string(), |s| s.to_string()));
                    }
                }
            }
            Err(e) => eprintln!("error: checking the Workshop: {e:#}"),
        }
        std::thread::sleep(std::time::Duration::from_secs(every.max(1) * 60));
    }
}
