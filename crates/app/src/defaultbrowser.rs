//! The default browser of this computer and its version (#668), found once when ODM starts, on a
//! thread of its own (reading a version can start a program: `--version`, bounded in time): its
//! User-Agent becomes the one ODM sends for links added by hand (`odm_engine::agent`). Where to
//! look is Windows’ registered default browser; command-line parsing is shared here.

use std::sync::mpsc;
use std::time::Duration;

use odm_engine::agent::Found;

/// Look for the default browser on a thread of its own; the automatic User-Agent is a current
/// Chrome's until it is found. Returns a receiver told when it is done (`wait`).
pub fn start() -> mpsc::Receiver<()> {
    let (tx, rx) = mpsc::channel();
    let spawned = std::thread::Builder::new()
        .name("default-browser".into())
        .spawn(move || {
            let started = std::time::Instant::now();
            let found = find();
            let browser = found.as_ref().map_or("none ODM knows".to_owned(), |f| {
                format!("{:?} {}", f.browser, f.version.as_deref().unwrap_or("?"))
            });
            odm_engine::agent::set_automatic(found);
            tracing::info!(
                "default browser: {browser} ({} ms); links added by hand go as {:?}",
                started.elapsed().as_millis(),
                odm_engine::agent::automatic()
            );
            let _ = tx.send(());
        });
    if spawned.is_err() {
        odm_engine::agent::set_automatic(None);
    }
    rx
}

/// Wait at most `limit` for `start`'s search (the command line's downloads begin at once).
pub fn wait(done: &mpsc::Receiver<()>, limit: Duration) {
    let _ = done.recv_timeout(limit);
}

fn find() -> Option<Found> {
    crate::win32::default_browser()
}

/// The program a Windows `shell\open\command` runs: `"C:\…\brave.exe" --single-argument %1`
/// (quoted), or `C:\Program Files\Mozilla Firefox\firefox.exe -osint -url "%1"` (not quoted: up
/// to its `.exe`).
// Windows' registry only (win32.rs); tested everywhere.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn windows_program(command: &str) -> Option<String> {
    let c = command.trim();
    let program = match c.strip_prefix('"') {
        Some(rest) => rest.split('"').next()?,
        None => {
            let end = c.to_ascii_lowercase().find(".exe")? + 4;
            &c[..end]
        }
    };
    let program = program.trim();
    (!program.is_empty()).then(|| program.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_commands_name_their_program() {
        assert_eq!(
            windows_program(
                r#""C:\Program Files\BraveSoftware\Brave-Browser\Application\brave.exe" --single-argument %1"#
            )
            .as_deref(),
            Some(r"C:\Program Files\BraveSoftware\Brave-Browser\Application\brave.exe")
        );
        assert_eq!(
            windows_program(r#"C:\Program Files\Mozilla Firefox\firefox.EXE -osint -url "%1""#)
                .as_deref(),
            Some(r"C:\Program Files\Mozilla Firefox\firefox.EXE")
        );
        assert_eq!(windows_program(""), None);
        assert_eq!(windows_program("rundll32 url.dll"), None);
    }
}
