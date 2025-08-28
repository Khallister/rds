use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

/// Log level numeric values (lower => more verbose)
const LEVEL_DEBUG: u8 = 1;
const LEVEL_INFO: u8 = 2;
const LEVEL_OFF: u8 = 255;

static ENABLED: AtomicBool = AtomicBool::new(false);
static LEVEL: AtomicU8 = AtomicU8::new(LEVEL_OFF);

#[cfg(test)]
use std::sync::Mutex;

#[cfg(test)]
static TEST_LAST: Mutex<Option<String>> = Mutex::new(None);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Debug,
    Info,
}

fn level_to_u8(l: Level) -> u8 {
    match l {
        Level::Debug => LEVEL_DEBUG,
        Level::Info => LEVEL_INFO,
    }
}

/// Initialize the logger enabled state (call early, e.g. from main).
/// If the environment variable `RDS_LOG=debug` is set, the level will be Debug.
pub fn init(enabled: bool) {
    ENABLED.store(enabled, Ordering::SeqCst);
    if enabled {
        // default to INFO unless env requests DEBUG
        let lvl = match std::env::var("RDS_LOG") {
            Ok(v) if v.to_lowercase() == "debug" => Level::Debug,
            _ => Level::Info,
        };
        LEVEL.store(level_to_u8(lvl), Ordering::SeqCst);
    } else {
        LEVEL.store(LEVEL_OFF, Ordering::SeqCst);
    }
}

/// Returns whether logging is enabled (any level)
pub fn enabled() -> bool {
    ENABLED.load(Ordering::SeqCst) && LEVEL.load(Ordering::SeqCst) != LEVEL_OFF
}

/// Returns the currently configured level
#[allow(dead_code)]
pub fn level() -> Option<Level> {
    match LEVEL.load(Ordering::SeqCst) {
        LEVEL_DEBUG => Some(Level::Debug),
        LEVEL_INFO => Some(Level::Info),
        _ => None,
    }
}

/// Returns true if a message at the given level should be logged.
///
/// Note: Lower numeric values represent more verbose levels (e.g., Debug = 1, Info = 2).
/// A message is logged if its level is as "important" (numerically >=) as the current level.
fn should_log(at: Level) -> bool {
    if !ENABLED.load(Ordering::SeqCst) {
        return false;
    }
    let cur = LEVEL.load(Ordering::SeqCst);
    level_to_u8(at) >= cur
}

fn write(level: &str, msg: &str) {
    let out = format!("[rds::{}] {}", level, msg);
    eprintln!("{}", out);
    #[cfg(test)]
    {
        // store last log message for tests; avoid panics on poisoned mutex
        if let Ok(mut lock) = TEST_LAST.lock() {
            *lock = Some(out);
        }
    }
}

#[cfg(test)]
/// Return last logged message (test-only helper)
pub fn test_last_log() -> Option<String> {
    if let Ok(lock) = TEST_LAST.lock() {
        lock.clone()
    } else {
        None
    }
}

/// Log an info-level message
pub fn info(msg: &str) {
    if should_log(Level::Info) {
        write("INFO", msg);
    }
}

/// Log a debug-level message
pub fn debug(msg: &str) {
    if should_log(Level::Debug) {
        write("DEBUG", msg);
    }
}

/// Log a formatted message at info level (convenience)
#[allow(dead_code)]
pub fn info_fmt(args: std::fmt::Arguments) {
    if should_log(Level::Info) {
        write("INFO", &format!("{}", args));
    }
}

/// Log a formatted message at debug level (convenience)
#[allow(dead_code)]
pub fn debug_fmt(args: std::fmt::Arguments) {
    if should_log(Level::Debug) {
        write("DEBUG", &format!("{}", args));
    }
}

#[cfg(test)]
mod tests;
