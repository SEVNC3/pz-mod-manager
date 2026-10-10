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
}

#[derive(Subcommand)]
enum ProfileCmd {
    List,
    /// Create a profile from the current one
    New { name: String },
    /// Switch to a profile and apply it to the game
    Use { name: String },
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
    let (mut p, adopted) = profile::sync(&paths)?;
    if adopted {
        println!("picked up mod list changes made in-game (profile {:?})", p.name);
    }
    match cli.cmd {
        Cmd::Add { sources: s, no_enable } => {
            let parsed: Vec<Source> = s.iter().map(|s| Source::parse(s)).collect();
            let ids = sources::install(&paths, &parsed);
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
            sources::install(&paths, &all);
        }
        Cmd::Rollback { id } => {
            store::rollback(&paths, &id)?;
            println!("{id}: restored previous version");
        }
        Cmd::List => list(&paths, &p)?,
        Cmd::Profile(ProfileCmd::List) => {
            for name in profile::list(&paths) {
                println!("{} {name}", if name == p.name { '*' } else { ' ' });
            }
        }
        Cmd::Profile(ProfileCmd::New { name }) => {
            if profile::list(&paths).contains(&name) {
                bail!("profile {name:?} already exists");
            }
            profile::save(&paths, &Profile { name: name.clone(), lines: p.lines })?;
            println!("created {name:?} (switch with: pzmm profile use {name})");
        }
        Cmd::Profile(ProfileCmd::Use { name }) => {
            profile::apply(&paths, &profile::load(&paths, &name)?)?;
            println!("now using {name:?}");
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
