//! Which mods the game's Lua errors come from. Each error in `Zomboid/console.txt` has a
//! STACK TRACE block whose frames read
//! `function: render -- file: X.lua line # 178 | MOD: Mod Name` (or `| Vanilla`).
//! The first frame of a block that names a mod is the likely culprit.

use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, PartialEq, serde::Serialize)]
pub struct ModErrors {
    /// The mod's display name as the game logs it.
    pub name: String,
    pub count: usize,
    /// The first offending frame, e.g. `render -- file: X.lua line # 178`.
    pub example: String,
}

/// Lua errors per mod in a console log, most first. Vanilla-only traces are skipped.
pub fn lua_errors(text: &str) -> Vec<ModErrors> {
    let mut by_mod: BTreeMap<String, ModErrors> = BTreeMap::new();
    let mut open = false; // inside a STACK TRACE block that hasn't been attributed yet
    for line in text.lines() {
        if line.contains("STACK TRACE") {
            open = true;
            continue;
        }
        let frame_line = line.trim_start();
        if frame_line.starts_with("LOG") || frame_line.starts_with("ERROR") || frame_line.starts_with("WARN") {
            open = false; // the trace ended without naming a mod
        }
        if !open || !frame_line.starts_with("function:") {
            continue;
        }
        if let Some((frame, name)) = line.split_once("| MOD:") {
            let name = name.trim().to_string();
            let frame = frame.trim().trim_start_matches("function:").trim().to_string();
            let e = by_mod.entry(name.clone()).or_insert(ModErrors { name, count: 0, example: frame });
            e.count += 1;
            open = false;
        }
    }
    let mut out: Vec<ModErrors> = by_mod.into_values().collect();
    out.sort_by(|a, b| b.count.cmp(&a.count).then(a.name.cmp(&b.name)));
    out
}

/// `Zomboid/console.txt` (the current or last game session).
pub fn console_errors(zomboid: &Path) -> Vec<ModErrors> {
    let bytes = std::fs::read(zomboid.join("console.txt")).unwrap_or_default();
    lua_errors(&String::from_utf8_lossy(&bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attributes_each_trace_to_its_first_mod_frame() {
        let log = "\
ERROR: General f:0> ExceptionLogger.logException> Exception thrown java.lang.RuntimeException: attempted index: size of non-table: null
-----------------------------------------
STACK TRACE
-----------------------------------------
function: render -- file: RasFarmingMenu.lua line # 178 | MOD: ra's Farming & Canning
function: onTick -- file: Other.lua line # 3 | MOD: Other Mod
ERROR: General f:1> again
STACK TRACE
-----------------------------------------
function: update -- file: ISSearchManager.lua line # 1255 | Vanilla
function: render -- file: RasFarmingMenu.lua line # 180 | MOD: ra's Farming & Canning
STACK TRACE
function: update -- file: ISSearchManager.lua line # 1255 | Vanilla
LOG  : General a normal line -- file: nothing | MOD: not a trace
";
        let errs = lua_errors(log);
        assert_eq!(errs.len(), 1, "{errs:?}");
        assert_eq!(errs[0].name, "ra's Farming & Canning");
        assert_eq!(errs[0].count, 2);
        assert_eq!(errs[0].example, "render -- file: RasFarmingMenu.lua line # 178");
    }
}
