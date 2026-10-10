use anyhow::{Result, bail};
use clap::{Parser, Subcommand};
use pzmm_core::profile::{self, Profile};
use pzmm_core::pz::Paths;
use pzmm_core::sources::{self, Source};
use pzmm_core::store;

/// PZMM - Project Zomboid Mod Manager
#[derive(Parser)]
#[command(version, after_help = "Env: PZ_HOME (default ~/Zomboid), PZMM_DATA, GITHUB_TOKEN")]
struct Cli {
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
    /// Re-download everything PZMM installed
    Update,
    /// Restore a mod's previous version (run again to undo)
    Rollback { id: String },
    /// Manage profiles (named mod setups)
    #[command(subcommand)]
    Profile(ProfileCmd),
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
    /// Lua errors in the last game session (console.txt), counted per mod
    Errors,
    /// List mods PZMM installed from the Workshop that have updates waiting
    Outdated,
    /// Server configs: export (profile -> Server/<name>.ini), import (ini -> profile) or start
    Server {
        /// export, import or start
        action: String,
        name: String,
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
}

#[derive(Subcommand)]
enum ProfileCmd {
    List,
    /// Create a profile from the current one
    New { name: String },
    /// Switch to a profile and apply it to the game
    Use { name: String },
    /// Write the active profile as a shareable JSON file (stdout without a file)
    Export { file: Option<std::path::PathBuf> },
    /// Install what a shared profile needs from the Workshop, then use it
    Import { file: std::path::PathBuf },
    Delete { name: String },
}

fn main() {
    if let Err(e) = run(Cli::parse()) {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<()> {
    let paths = Paths::detect();
    // read-only commands skip the lock; the rest wait for the app to finish its change
    let _lock = if matches!(cli.cmd, Cmd::Search { .. } | Cmd::Watch { .. }) { None } else { Some(pzmm_core::lock_data(&paths)?) };
    let (mut p, adopted) = profile::sync(&paths)?;
    if adopted {
        println!("picked up mod list changes made in-game (profile {:?})", p.name);
    }
    match cli.cmd {
        Cmd::Add { sources: s, no_enable } => {
            let parsed: Vec<Source> = s.iter().map(|s| Source::parse(s)).collect();
            let ids = sources::install(&paths, &parsed, true);
            if !no_enable && !ids.is_empty() {
                ids.iter().for_each(|i| p.set(i, true));
                profile::apply(&paths, &p)?;
            }
        }
        Cmd::Enable { ids } => {
            ids.iter().for_each(|i| p.set(i, true));
            profile::apply(&paths, &p)?;
        }
        Cmd::Disable { ids } => {
            ids.iter().for_each(|i| p.set(i, false));
            profile::apply(&paths, &p)?;
        }
        Cmd::Remove { ids } => {
            let refused = store::remove(&paths, &ids)?;
            for id in &refused {
                eprintln!("{id}: not installed by PZMM (Workshop subscription or manual install); left alone");
            }
            ids.iter().filter(|i| !refused.contains(i)).for_each(|i| p.forget(i));
            profile::apply(&paths, &p)?;
        }
        Cmd::Update => {
            let mut seen = std::collections::HashSet::new();
            let all: Vec<Source> = store::load(&paths)?
                .into_values()
                .filter(|e| seen.insert(e.source.clone()))
                .filter_map(|e| Source::from_index(&e.source))
                .collect();
            sources::install(&paths, &all, false);
        }
        Cmd::Rollback { id } => {
            store::rollback(&paths, &id)?;
            println!("{id}: restored previous version");
        }
        Cmd::List => list(&paths, &p)?,
        Cmd::Search { query, sort, page, build } => {
            let tag = build.map(|b| format!("Build {b}"));
            let res = sources::steam_workshop::browse(pzmm_core::pz::STEAM_APP, &query.join(" "), &sort, page, tag.as_deref())?;
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
                    ids.iter().for_each(|i| p.set(i, true));
                    profile::apply(&paths, &p)?;
                }
                _ if install => bail!("no Workshop item clearly provides {mod_id}; install one of the above by id"),
                _ => {}
            }
        }
        Cmd::Doctor => {
            use pzmm_core::pz::doctor;
            let found = doctor::check(&paths, &p.enabled(), &store::scan(&paths)?, profile::effective_build(&paths));
            for f in &found {
                let tag = if f.level == doctor::Level::Error { "error  " } else { "warning" };
                println!("{tag} {}: {}", f.subject, f.text);
            }
            let errors = found.iter().filter(|f| f.level == doctor::Level::Error).count();
            println!("{errors} error(s), {} warning(s)", found.len() - errors);
            if errors > 0 {
                std::process::exit(1);
            }
        }
        Cmd::Errors => {
            let errs = pzmm_core::pz::logs::console_errors(&paths.zomboid);
            if errs.is_empty() {
                println!("no mod errors in {}", paths.zomboid.join("console.txt").display());
            }
            for e in errs {
                println!("{:>5}  {}  (first: {})", e.count, e.name, e.example);
            }
        }
        Cmd::Outdated => {
            let ids = sources::check_updates(&paths)?;
            if ids.is_empty() {
                println!("everything PZMM installed is up to date");
            }
            ids.iter().for_each(|i| println!("{i}: update available (pzmm update)"));
        }
        Cmd::Server { action, name } => server(&paths, &p, &action, &name)?,
        Cmd::Watch { server, every, on_update } => watch(&paths, &server, every, on_update.as_deref())?,
        Cmd::Mode { to, dry_run } => {
            use pzmm_core::pz::viewpoint as vp;
            let Some(to) = to else {
                println!("{} (profile {:?})", profile::kind_of(&paths, &p.name), p.name);
                return Ok(());
            };
            if to != vp::KIND && to != profile::DEFAULT_KIND {
                bail!("expected zomboid or viewpoint");
            }
            for line in vp::preview(&paths, &to, &store::scan(&paths)?)? {
                println!("{} {line}", if dry_run { "would" } else { "-" });
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
            for name in profile::list(&paths) {
                println!("{} {name}", if name == p.name { '*' } else { ' ' });
            }
        }
        Cmd::Profile(ProfileCmd::New { name }) => {
            if profile::exists(&paths, &name) {
                bail!("profile {name:?} already exists");
            }
            profile::save(&paths, &Profile { name: name.clone(), lines: p.lines })?;
            println!("created {name:?} (switch with: pzmm profile use {name})");
        }
        Cmd::Profile(ProfileCmd::Use { name }) => {
            profile::apply(&paths, &profile::load(&paths, &name)?)?;
            println!("now using {name:?}");
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
            sources::install(&paths, &profile::import_needs(&shared, &store::scan(&paths)?), false);
            let p = profile::import(&paths, &shared)?;
            println!("now using {:?}", p.name);
        }
        Cmd::Profile(ProfileCmd::Delete { name }) => profile::delete(&paths, &name)?,
        Cmd::Build { build } => profile::set_b41(&paths, Some(match build {
            41 => true,
            42 => false,
            _ => bail!("build must be 41 or 42"),
        }))
        .and_then(|_| profile::apply(&paths, &p))?,
    }
    Ok(())
}

fn list(paths: &Paths, p: &Profile) -> Result<()> {
    let mut mods = store::scan(paths)?;
    println!("profile: {}", p.name);
    for line in &p.lines {
        match line {
            profile::Line::Separator(s) => println!("    --- {s} ---"),
            profile::Line::Mod { id, on } => match mods.remove(id) {
                Some(m) => println!("[{}] {:30} {:40} {}", if *on { 'x' } else { ' ' }, m.id, m.name, m.source),
                None if *on => println!("[x] {id:30} {:40}", "(missing)"),
                None => {}
            },
        }
    }
    for m in mods.values() {
        println!("[ ] {:30} {:40} {}", m.id, m.name, m.source);
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
                println!("{c}");
            }
            p.set(zb::MOD_ID, true);
            profile::apply(paths, p)?;
            println!("ZombieBuddy is set up. Java mods will ask for your approval the first time they load.");
        }
        "uninstall" => {
            let Some(l) = launcher else { bail!("Project Zomboid's game folder wasn't found; set PZ_GAME_DIR") };
            for c in zb::uninstall(&l, false)? {
                println!("{c}");
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
            for note in server::export(paths, name, &p.enabled(), &store::scan(paths)?)? {
                println!("{note}");
            }
        }
        "import" => {
            let cfg = server::read(&file)?;
            let have = sources::workshop_available(paths);
            let missing: Vec<Source> = cfg.workshop.iter().filter(|w| !have.contains(*w)).map(|w| Source::Workshop(w.clone())).collect();
            sources::install(paths, &missing, false);
            let prof = Profile::from_ids(&format!("Server {name}"), cfg.mods);
            profile::apply(paths, &prof)?;
            println!("now using profile {:?}", prof.name);
        }
        "start" => println!("started {name} with {}", server::start(paths, name)?.display()),
        _ => bail!("expected export, import or start"),
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
        let cfg = server::read(&file)?;
        let ids: Vec<&str> = cfg.workshop.iter().map(String::as_str).collect();
        match steam_workshop::details(&ids) {
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
