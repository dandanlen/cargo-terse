use std::ffi::OsString;
use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};

use crate::cli::{OutputFormat, Verbosity};
use crate::format;
use crate::runner;

pub fn run_watch(
    cargo_cmd: &str,
    cargo_args: &[OsString],
    format: &OutputFormat,
    verbosity: &Verbosity,
    no_cache: bool,
) -> i32 {
    let formatter = format::create_formatter(format, verbosity);

    // Initial run.
    clear_screen();
    let output = runner::execute_cargo(cargo_cmd, cargo_args);
    let result = runner::display_results(&output, &*formatter);
    if !no_cache {
        crate::cache::write_cache(&result);
    }
    show_watching_status(cargo_cmd);

    // Set up notify watcher. Watch only paths that actually exist.
    let (tx, rx) = mpsc::channel();
    let mut watcher = match RecommendedWatcher::new(
        move |res| {
            let _ = tx.send(res);
        },
        Config::default(),
    ) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("cargo-terse: failed to create watcher: {e}");
            return 1;
        }
    };

    for path in watch_paths() {
        if path.exists() {
            let mode = if path.is_dir() {
                RecursiveMode::Recursive
            } else {
                RecursiveMode::NonRecursive
            };
            if let Err(e) = watcher.watch(&path, mode) {
                eprintln!("cargo-terse: warning: could not watch {}: {e}", path.display());
            }
        }
    }

    // Event loop.
    loop {
        // Block until the first event arrives.
        match rx.recv() {
            Ok(Ok(event)) => {
                if !event_is_relevant(&event) {
                    continue;
                }
            }
            Ok(Err(_)) | Err(_) => continue,
        }

        // Drain any further events within the debounce window.
        debounce(&rx, Duration::from_millis(300));

        clear_screen();
        let output = runner::execute_cargo(cargo_cmd, cargo_args);
        let result = runner::display_results(&output, &*formatter);
        if !no_cache {
            crate::cache::write_cache(&result);
        }
        show_watching_status(cargo_cmd);
    }
}

fn watch_paths() -> Vec<std::path::PathBuf> {
    vec![
        Path::new("src").to_path_buf(),
        Path::new("tests").to_path_buf(),
        Path::new("benches").to_path_buf(),
        Path::new("examples").to_path_buf(),
        Path::new("build.rs").to_path_buf(),
        Path::new("Cargo.toml").to_path_buf(),
    ]
}

fn event_is_relevant(event: &notify::Event) -> bool {
    event.paths.iter().any(|p| {
        p.extension()
            .and_then(|e| e.to_str())
            .map(|e| matches!(e, "rs" | "toml" | "lock"))
            .unwrap_or(false)
    })
}

fn debounce(rx: &mpsc::Receiver<notify::Result<notify::Event>>, window: Duration) {
    let deadline = std::time::Instant::now() + window;
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        match rx.recv_timeout(remaining) {
            Ok(_) => {} // drain and keep waiting
            Err(_) => break,
        }
    }
}

fn clear_screen() {
    print!("\x1b[2J\x1b[H");
}

fn show_watching_status(cargo_cmd: &str) {
    // Dim text: ESC[2m ... ESC[0m
    println!("\x1b[2m[watching for changes... cargo {cargo_cmd}]\x1b[0m");
}
