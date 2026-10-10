//! PZMM core. `pz` holds every Project Zomboid specific; the rest is game-neutral.

pub mod profile;
pub mod pz;
pub mod sources;
pub mod store;

use std::sync::OnceLock;

type Sink = Box<dyn Fn(&str) + Send + Sync>;
static SINK: OnceLock<Sink> = OnceLock::new();

/// Send progress/error lines somewhere other than stdout (the desktop app's activity log).
pub fn set_log_sink(f: impl Fn(&str) + Send + Sync + 'static) {
    let _ = SINK.set(Box::new(f));
}

pub fn log_line(msg: &str) {
    match SINK.get() {
        Some(f) => f(msg),
        None => println!("{msg}"),
    }
}

#[macro_export]
macro_rules! log {
    ($($t:tt)*) => { $crate::log_line(&format!($($t)*)) };
}
