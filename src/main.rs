mod cache;
mod cli;
mod diagnostic;
mod format;
mod parser;
mod runner;

fn main() {
    let args: Vec<std::ffi::OsString> = std::env::args_os().collect();

    if args.iter().any(|a| a == "--version") {
        println!("cargo-terse {}", env!("CARGO_PKG_VERSION"));
        return;
    }

    match cli::parse_args(args) {
        Ok(cmd) => match cmd {
            cli::Command::Run { cargo_cmd, format, verbosity, no_cache, cargo_args } => {
                std::process::exit(runner::run_cargo(&cargo_cmd, &cargo_args, &format, &verbosity, no_cache));
            }
            cli::Command::Detail { id, format } => {
                match cache::lookup_diagnostic(&id) {
                    Some(diag) => {
                        println!("{}", format::create_formatter(&format, &cli::Verbosity::VeryVerbose).format_diagnostic(&diag));
                    }
                    None => {
                        eprintln!("cargo-terse: no cached diagnostic with id '{id}'");
                        eprintln!("hint: run a cargo terse command first");
                        std::process::exit(1);
                    }
                }
            }
        },
        Err(e) => {
            eprintln!("cargo-terse: {e}");
            std::process::exit(2);
        }
    }
}
